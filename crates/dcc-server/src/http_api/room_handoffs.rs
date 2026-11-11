//! Passing the turn from the phone's room: an agent's answer that ends with
//! "@Teammate …" (`dcc_core::domain::team_room::handoff`) and that DCC did
//! not pass on its own asks the person "Passar a vez" or "Ignorar", as the
//! desktop's room does. What the person did is kept in the desktop's own
//! `room-handoffs.json` (by the answer's key), read and written here the
//! way `mobile_rooms` reads `agent-rooms.json`, so neither asks again.

use std::collections::BTreeMap;
use std::path::Path;

use dcc_core::domain::agent::ResidentAgent;
use dcc_core::domain::agent_chain::passed_answer;
use dcc_core::domain::team_room::{handoff, handoff_prompt, RouteReason, Teammate};
use dcc_core::domain::team_room_lines::{RoomLine, Speaker};
use dcc_infra::db::SqliteSessionRepo;

use super::mobile_room::room_param;
use super::mobile_room_send::{deliver, destination_of, room_name, sent_json, Destination};
use super::room_feed::{read_team, room_lines};
use super::*;

const FILE: &str = "room-handoffs.json";
/// The desktop keeps the newest this many.
const MAX_ENTRIES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Handled {
    Passed,
    Ignored,
}

/// By answer key: what the person did and when.
pub(super) type HandledMap = BTreeMap<String, (Handled, String)>;

/// The desktop's file; empty when missing or unreadable.
pub(super) fn load(app_data: &Path) -> HandledMap {
    std::fs::read(app_data.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Keeps what the person did with the answer `key`, the newest
/// `MAX_ENTRIES` as the desktop does.
pub(super) fn save(app_data: &Path, key: &str, handled: Handled) -> Result<(), String> {
    let mut map = load(app_data);
    map.insert(key.to_string(), (handled, chrono::Utc::now().to_rfc3339()));
    if map.len() > MAX_ENTRIES {
        let mut entries: Vec<_> = std::mem::take(&mut map).into_iter().collect();
        entries.sort_by(|left, right| right.1 .1.cmp(&left.1 .1));
        entries.truncate(MAX_ENTRIES);
        map = entries.into_iter().collect();
    }
    let bytes = serde_json::to_vec(&map).map_err(|error| error.to_string())?;
    std::fs::write(app_data.join(FILE), bytes).map_err(|error| error.to_string())
}

/// What moves the room's lines when the person acts on a suggestion.
pub(super) fn digest(map: &HandledMap, digest: &mut Sha256) {
    for (key, (handled, _)) in map {
        digest.update(key.as_bytes());
        digest.update([*handled as u8]);
    }
}

/// The teammate an agent's answer suggests, and its request.
pub(super) fn suggestion<'a>(
    line: &RoomLine,
    agents: &'a [ResidentAgent],
) -> Option<(&'a ResidentAgent, String)> {
    if line.speaker != Speaker::Agent {
        return None;
    }
    let team: Vec<Teammate<'_>> = agents
        .iter()
        .map(|agent| Teammate {
            id: &agent.id,
            name: &agent.name,
            preset: agent.preset.as_deref(),
        })
        .collect();
    let (mate, request) = handoff(&line.text, &team, &line.agent_id)?;
    let agent = agents.iter().find(|agent| agent.id == mate.id)?;
    Some((agent, request))
}

/// `{to, toName, request, state}` under an answer that suggests a teammate:
/// `pending` (the bar asks), `passed` or `auto` (DCC passed it itself);
/// nothing once ignored.
pub(super) fn handoff_json(
    line: &RoomLine,
    agents: &[ResidentAgent],
    handled: &HandledMap,
    lines: &[RoomLine],
) -> Option<Value> {
    let (to, request) = suggestion(line, agents)?;
    let auto = lines.iter().any(|other| {
        other.speaker == Speaker::Person && passed_answer(&other.text) == Some(line.key.as_str())
    });
    let state = match (auto, handled.get(&line.key).map(|(handled, _)| *handled)) {
        (true, _) => "auto",
        (false, Some(Handled::Ignored)) => return None,
        (false, Some(Handled::Passed)) => "passed",
        (false, None) => "pending",
    };
    Some(json!({
        "to": to.id,
        "toName": to.name,
        "request": request,
        "state": state,
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct HandoffBody {
    /// The answer that suggests it (its line's `key`).
    key: String,
    /// `pass` or `ignore`.
    action: String,
    #[serde(default)]
    room: Option<String>,
}

/// Where the suggestion of the answer `key` goes, and its message.
fn passing(
    db_path: &Path,
    room: Option<&str>,
    key: &str,
) -> Result<Result<(Destination, String), HttpApiError>, String> {
    let room_name = match room_name(db_path, room)? {
        Ok(name) => name,
        Err(refused) => return Ok(Err(refused)),
    };
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let team = read_team(&repo)?;
    let lines = room_lines(&repo, &team.room_homes(room))?;
    let found = lines.iter().find(|line| line.key == key).and_then(|line| {
        let (to, request) = suggestion(line, &team.agents)?;
        let from = team.agents.iter().find(|agent| agent.id == line.agent_id)?;
        Some((
            to,
            handoff_prompt(&from.name, &to.name, &request, &line.text),
        ))
    });
    let Some((to, prompt)) = found else {
        return Ok(Err(HttpApiError::Refused(
            "roomNoHandoff",
            "that answer suggests no teammate".to_string(),
            json!({ "key": key }),
        )));
    };
    let session = team.conversation(&to.id, room);
    Ok(Ok((
        destination_of(&team, to, RouteReason::Mention, session, room_name),
        prompt,
    )))
}

/// `POST /api/v1/mobile/room/handoff`: `{key, action, room?}` — `pass`
/// sends the suggestion to the teammate's conversation in that room
/// (started the first time, queued while it works) with what the first
/// agent said; `ignore` only keeps the choice.
pub(super) async fn act(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Json(body): Json<HandoffBody>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let app_data = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let room = room_param(body.room);
    let key = body.key.trim().to_string();
    if key.is_empty() {
        return Err(HttpApiError::bad_request("no answer named".to_string()));
    }
    match body.action.as_str() {
        "ignore" => {
            save(&app_data, &key, Handled::Ignored).map_err(HttpApiError::internal)?;
            Ok(Json(json!({ "ignored": true })))
        }
        "pass" => {
            let (to, prompt) = {
                let (room, key) = (room.clone(), key.clone());
                tokio::task::spawn_blocking(move || passing(&db_path, room.as_deref(), &key))
                    .await
                    .map_err(|error| HttpApiError::internal(error.to_string()))?
                    .map_err(HttpApiError::internal)??
            };
            let delivered = deliver(config, &backend, &to, room.as_deref(), &prompt, None).await?;
            save(&app_data, &key, Handled::Passed).map_err(HttpApiError::internal)?;
            Ok(Json(sent_json(&to, &delivered, room.as_deref())))
        }
        other => Err(HttpApiError::bad_request(format!(
            "unknown handoff action {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(key: &str, agent: &str, speaker: Speaker, text: &str) -> RoomLine {
        RoomLine {
            key: key.into(),
            agent_id: agent.into(),
            session_id: "s".into(),
            speaker,
            text: text.into(),
            at: "2026-11-10T10:00:00Z".into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    #[test]
    fn an_answer_that_names_a_teammate_asks_until_the_person_acts() {
        let agents = vec![
            super::super::room_feed::tests::agent("dev"),
            super::super::room_feed::tests::agent("ops"),
        ];
        let answer = said(
            "s:t1",
            "dev",
            Speaker::Agent,
            "Corrigi o cache.\n\n@Agente ops pode subir em staging?",
        );
        let lines = vec![answer.clone()];
        let mut handled = HandledMap::new();
        let value = handoff_json(&answer, &agents, &handled, &lines).unwrap();
        assert_eq!(value["to"], "ops");
        assert_eq!(value["toName"], "Agente ops");
        assert_eq!(value["request"], "@Agente ops pode subir em staging?");
        assert_eq!(value["state"], "pending");
        handled.insert("s:t1".into(), (Handled::Passed, "x".into()));
        assert_eq!(
            handoff_json(&answer, &agents, &handled, &lines).unwrap()["state"],
            "passed"
        );
        handled.insert("s:t1".into(), (Handled::Ignored, "x".into()));
        assert!(handoff_json(&answer, &agents, &handled, &lines).is_none());
        // Passed by DCC itself (the teammate's line names the answer).
        let auto = dcc_core::domain::agent_chain::auto_handoff_prompt(
            "Agente dev",
            "Agente ops",
            "@Agente ops pode subir em staging?",
            &answer.text,
            "s:t1",
            1,
        );
        let lines = vec![answer.clone(), said("o:1", "ops", Speaker::Person, &auto)];
        assert_eq!(
            handoff_json(&answer, &agents, &HandledMap::new(), &lines).unwrap()["state"],
            "auto"
        );
        // Naming itself, or the person's own words, suggests nothing.
        let own = said("s:t2", "dev", Speaker::Agent, "@Agente dev faço depois");
        assert!(handoff_json(&own, &agents, &HandledMap::new(), &[]).is_none());
        let person = said("s:p", "dev", Speaker::Person, "@Agente ops oi");
        assert!(suggestion(&person, &agents).is_none());
    }

    #[test]
    fn the_choice_is_kept_in_the_desktops_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_empty());
        save(dir.path(), "s:t1", Handled::Passed).unwrap();
        save(dir.path(), "s:t2", Handled::Ignored).unwrap();
        let map = load(dir.path());
        assert_eq!(map["s:t1"].0, Handled::Passed);
        assert_eq!(map["s:t2"].0, Handled::Ignored);
        // The desktop's own shape: `{"key": ["passed", "<time>"]}`.
        let raw: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(FILE)).unwrap()).unwrap();
        assert_eq!(raw["s:t1"][0], "passed");
        let (mut one, mut two) = (Sha256::new(), Sha256::new());
        digest(&map, &mut one);
        digest(&HandledMap::new(), &mut two);
        assert_ne!(one.finalize(), two.finalize());
    }
}
