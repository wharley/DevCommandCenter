//! A room on the phone (`dcc_core::domain::team_room`): Geral, or with
//! `room` a project's room — the latest lines of every agent's own
//! conversation held there together in time, read from DCC's database like
//! the desktop's room, with the approvals and the questions its
//! conversations wait on and who works there now. A phone that sends back the `snapshotId` it holds as
//! `after` gets `{unchanged, snapshotId}` without a single event read while
//! no conversation of the room moved. The message the phone writes goes
//! through `mobile_room_send`.
//!
//! While an agent works the room also carries `live` (each turn at work in
//! the room's conversations and in the tasks they opened, `room_turns`),
//! `stopped` and `queued`; the `snapshotId` then moves with what streams,
//! read cheaply from those conversations' event stamps. `linesId` names the
//! settled part: a phone that sends it back as `lines` gets
//! `linesUnchanged` without the lines while only the turns at work moved.
//!
//! The room also carries `cards`, the ones the desktop's room shows there
//! (`room_cards`), and each answer the person's 👍 / 👎 (`reaction`); both
//! move the ids, so a reaction given on the desktop reaches the phone.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::attention::AttentionReason;
use dcc_core::domain::delegation::DelegationId;
use dcc_core::domain::room_approval::pending_approval;
use dcc_core::domain::room_question::pending_question;
use dcc_core::domain::task_report::is_task;
use dcc_core::domain::team_room_live::turn_now;
use dcc_core::ports::DelegationRepo;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

use super::room_feed::{read_team, room_lines, Team};
use super::room_handoffs::HandledMap;
use super::room_line_json::{approval_json, line_json, question_json, task_json};
use super::room_turns::{self, RunningTask};
use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct RoomQuery {
    /// The `snapshotId` the phone already holds.
    after: Option<String>,
    /// The project whose room it is; none: Geral.
    room: Option<String>,
    /// The `linesId` whose lines the phone already holds.
    lines: Option<String>,
}

/// The room a phone names: a project, or Geral when blank or absent.
pub(super) fn room_param(room: Option<String>) -> Option<String> {
    room.map(|room| room.trim().to_string())
        .filter(|room| !room.is_empty())
}

/// The conversations waiting on a permission or a question, which are read
/// for it.
fn asking<'a>(team: &Team, homes: &[&'a AgentSessionBinding]) -> Vec<&'a AgentSessionBinding> {
    homes
        .iter()
        .filter(|home| {
            matches!(
                team.waiting(home),
                Some(AttentionReason::Permission | AttentionReason::Input)
            )
        })
        .copied()
        .collect()
}

/// What moves the room's settled part: the team, its conversations' turns
/// starting and ending, their requests and the handoffs the person acted on.
fn lines_id(
    repo: &SqliteSessionRepo,
    team: &Team,
    homes: &[&AgentSessionBinding],
    handled: &HandledMap,
) -> Result<String, String> {
    let mut digest = Sha256::new();
    team.digest(homes, &mut digest);
    // A second request of the same turn, or one of two answered, moves
    // only the history: read its stamp for the conversations asking
    // (a permission or a question).
    for home in asking(team, homes) {
        let (count, sequence, row) = repo
            .session_event_stamp(&SessionId(home.session_id.clone()))
            .map_err(|error| error.to_string())?;
        digest.update(format!("{count}:{sequence}:{row}").as_bytes());
    }
    super::room_handoffs::digest(handled, &mut digest);
    // A 👍 or 👎 given here or on the desktop.
    for home in homes {
        for (key, reaction) in repo
            .session_reactions(&home.session_id)
            .map_err(|error| error.to_string())?
        {
            digest.update(format!("{key}={}", reaction.key()).as_bytes());
        }
    }
    Ok(hex(&digest.finalize()[..12]))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The whole room's id: its settled part and every conversation at work
/// (`running`), by its history's stamp — one cheap read each.
fn snapshot_id(
    repo: &SqliteSessionRepo,
    lines_id: &str,
    running: &[&str],
    cards: &[Value],
) -> Result<String, String> {
    if running.is_empty() && cards.is_empty() {
        return Ok(lines_id.to_string());
    }
    let mut digest = Sha256::new();
    digest.update(lines_id.as_bytes());
    super::room_cards::digest(cards, &mut digest);
    for session in running {
        let (count, sequence, row) = repo
            .session_event_stamp(&SessionId(session.to_string()))
            .map_err(|error| error.to_string())?;
        digest.update(format!("{session}:{count}:{sequence}:{row}").as_bytes());
    }
    Ok(hex(&digest.finalize()[..12]))
}

/// The room's turns now: who is at work (`live`), the stops of the last
/// day that may be tried again (`stopped`) and the messages queued
/// (`queued`).
fn turns(
    repo: &SqliteSessionRepo,
    team: &Team,
    homes: &[&AgentSessionBinding],
    tasks: &[RunningTask],
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>), String> {
    let (mut live, mut stopped, mut queued) = (Vec::new(), Vec::new(), Vec::new());
    let now = chrono::Utc::now();
    for home in homes {
        if team.running(home) {
            let (turn, queue) = room_turns::now_and_queue(repo, &home.session_id)?;
            if let Some(turn) = turn.live() {
                live.push(room_turns::conversation_live(
                    &home.agent_id,
                    &home.session_id,
                    turn,
                ));
            }
            queued.extend(room_turns::queued_json(&home.agent_id, &queue));
        } else {
            let messages = super::room_feed::messages(repo, &home.session_id)?;
            if let Some(turn) = turn_now(&messages).stopped() {
                stopped.extend(room_turns::stopped_json(
                    &home.agent_id,
                    &home.session_id,
                    turn,
                    now,
                ));
            }
        }
    }
    for task in tasks {
        let (turn, _) = room_turns::now_and_queue(repo, &task.session_id)?;
        if let Some(turn) = turn.live() {
            live.push(room_turns::task_live(&task.agent_id, task, turn));
        }
    }
    // An agent at work does not show its earlier stop.
    let working = team.working(homes);
    stopped.retain(|stop| {
        !working
            .iter()
            .any(|agent| stop["agentId"].as_str() == Some(agent.as_str()))
    });
    Ok((live, stopped, queued))
}

/// The tasks the lines report, by delegation id.
fn reported_tasks(
    repo: &SqliteSessionRepo,
    lines: &[dcc_core::domain::team_room_lines::RoomLine],
) -> Result<HashMap<String, Value>, String> {
    let mut tasks = HashMap::new();
    for id in lines.iter().flat_map(|line| &line.tasks) {
        if tasks.contains_key(id) {
            continue;
        }
        let delegation = tokio::runtime::Handle::current()
            .block_on(repo.get_delegation(&DelegationId(id.clone())))
            .map_err(|error| error.to_string())?;
        if let Some(delegation) = delegation.filter(is_task) {
            tasks.insert(id.clone(), task_json(&delegation));
        }
    }
    Ok(tasks)
}

/// The approvals and the questions the room's conversations wait on, one
/// of each per conversation as the desktop's room asks them; each history
/// is read once for both.
fn requests(
    repo: &SqliteSessionRepo,
    team: &Team,
    homes: &[&AgentSessionBinding],
) -> Result<(Vec<Value>, Vec<Value>), String> {
    let (mut approvals, mut questions) = (Vec::new(), Vec::new());
    for home in asking(team, homes) {
        let events = repo
            .list_events_by_session_sync(&SessionId(home.session_id.clone()))
            .map_err(|error| error.to_string())?;
        let name = team
            .agents
            .iter()
            .find(|agent| agent.id == home.agent_id)
            .map(|agent| agent.name.as_str())
            .unwrap_or_default();
        if let Some(request) = pending_approval(&events) {
            approvals.push(approval_json(
                &home.session_id,
                &home.agent_id,
                name,
                &request,
            ));
        }
        if let Some(request) = pending_question(&events) {
            questions.push(question_json(
                &home.session_id,
                &home.agent_id,
                name,
                &request,
            ));
        }
    }
    Ok((approvals, questions))
}

/// The person's 👍 / 👎 on the room's answers, by answer key.
fn reactions(
    repo: &SqliteSessionRepo,
    homes: &[&AgentSessionBinding],
) -> Result<HashMap<String, dcc_core::domain::agent_reaction::Reaction>, String> {
    let mut all = HashMap::new();
    for home in homes {
        all.extend(
            repo.session_reactions(&home.session_id)
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(all)
}

fn read_room(
    db_path: &Path,
    app_data: &Path,
    room: Option<&str>,
    after: Option<&str>,
    lines_after: Option<&str>,
) -> Result<Value, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let team = read_team(&repo)?;
    let homes = team.room_homes(room);
    let handled = super::room_handoffs::load(app_data);
    let lines_id = lines_id(&repo, &team, &homes, &handled)?;
    let tasks = room_turns::running_tasks(&repo, &homes)?;
    let running: Vec<&str> = homes
        .iter()
        .filter(|home| team.running(home))
        .map(|home| home.session_id.as_str())
        .chain(tasks.iter().map(|task| task.session_id.as_str()))
        .collect();
    let sources = super::room_cards::read_sources(db_path, app_data, &repo, &team)?;
    let cards = super::room_cards::cards(&team, &sources, room);
    let snapshot = snapshot_id(&repo, &lines_id, &running, &cards)?;
    if after == Some(snapshot.as_str()) {
        return Ok(json!({ "unchanged": true, "snapshotId": snapshot }));
    }
    let (approvals, questions) = requests(&repo, &team, &homes)?;
    let (live, stopped, queued) = turns(&repo, &team, &homes, &tasks)?;
    let mut value = json!({
        "snapshotId": snapshot,
        "linesId": lines_id,
        "room": room,
        "approvals": approvals,
        "questions": questions,
        "working": team.working(&homes),
        "live": live,
        "stopped": stopped,
        "queued": queued,
        "cards": cards,
    });
    if lines_after == Some(lines_id.as_str()) {
        value["linesUnchanged"] = json!(true);
        return Ok(value);
    }
    let lines = super::room_line_json::shown(room_lines(&repo, &homes)?);
    let tasks = reported_tasks(&repo, &lines)?;
    let reactions = reactions(&repo, &homes)?;
    let shown: Vec<Value> = lines
        .iter()
        .map(|line| {
            let mut json = line_json(line, &tasks);
            if line.speaker == dcc_core::domain::team_room_lines::Speaker::Agent {
                json["reaction"] = json!(reactions.get(&line.key).map(|reaction| reaction.key()));
            }
            if let Some(handoff) =
                super::room_handoffs::handoff_json(line, &team.agents, &handled, &lines)
            {
                json["handoff"] = handoff;
            }
            json
        })
        .collect();
    value["lines"] = json!(shown);
    Ok(value)
}

/// `GET /api/v1/mobile/room[?room=<projectId>]`
pub(super) async fn room(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<RoomQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let app_data = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let room = room_param(query.room);
    let value = tokio::task::spawn_blocking(move || {
        read_room(
            &db_path,
            &app_data,
            room.as_deref(),
            query.after.as_deref(),
            query.lines.as_deref(),
        )
    })
    .await
    .map_err(|error| HttpApiError::internal(error.to_string()))?
    .map_err(HttpApiError::internal)?;
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_room_is_geral() {
        assert_eq!(room_param(None), None);
        assert_eq!(room_param(Some("  ".into())), None);
        assert_eq!(
            room_param(Some(" checkout ".into())).as_deref(),
            Some("checkout")
        );
    }

    #[test]
    fn only_the_conversations_asking_the_person_are_read_for_it() {
        let mut team = super::super::room_feed::tests::team();
        team.activity.insert(
            "s-new".into(),
            super::super::room_feed::tests::input(true, Some("input")),
        );
        let homes = team.room_homes(Some("checkout"));
        let read: Vec<&str> = asking(&team, &homes)
            .iter()
            .map(|home| home.session_id.as_str())
            .collect();
        // A question (`input`) is read as a permission is.
        assert_eq!(read, ["s-new", "s-dev-room"]);
        team.activity.insert(
            "s-new".into(),
            super::super::room_feed::tests::input(true, None),
        );
        let homes = team.room_homes(Some("checkout"));
        assert_eq!(asking(&team, &homes).len(), 1);
    }
}
