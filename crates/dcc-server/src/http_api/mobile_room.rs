//! A room on the phone (`dcc_core::domain::team_room`): Geral, or with
//! `room` a project's room — the latest lines of every agent's own
//! conversation held there together in time, read from DCC's database like
//! the desktop's room, with the approvals and the questions its
//! conversations wait on and who works there now. A phone that sends back the `snapshotId` it holds as
//! `after` gets `{unchanged, snapshotId}` without a single event read while
//! no conversation of the room moved. The message the phone writes goes
//! through `mobile_room_send`.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::domain::attention::AttentionReason;
use dcc_core::domain::delegation::DelegationId;
use dcc_core::domain::room_approval::pending_approval;
use dcc_core::domain::room_question::pending_question;
use dcc_core::domain::task_report::is_task;
use dcc_core::ports::DelegationRepo;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

use super::room_feed::{read_team, room_lines, Team};
use super::room_line_json::{approval_json, line_json, question_json, task_json};
use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct RoomQuery {
    /// The `snapshotId` the phone already holds.
    after: Option<String>,
    /// The project whose room it is; none: Geral.
    room: Option<String>,
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

fn snapshot_id(
    repo: &SqliteSessionRepo,
    team: &Team,
    homes: &[&AgentSessionBinding],
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
    Ok(digest.finalize()[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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

fn read_room(db_path: &Path, room: Option<&str>, after: Option<&str>) -> Result<Value, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let team = read_team(&repo)?;
    let homes = team.room_homes(room);
    let snapshot = snapshot_id(&repo, &team, &homes)?;
    if after == Some(snapshot.as_str()) {
        return Ok(json!({ "unchanged": true, "snapshotId": snapshot }));
    }
    let lines = super::room_line_json::shown(room_lines(&repo, &homes)?);
    let tasks = reported_tasks(&repo, &lines)?;
    let lines: Vec<Value> = lines.iter().map(|line| line_json(line, &tasks)).collect();
    let (approvals, questions) = requests(&repo, &team, &homes)?;
    Ok(json!({
        "snapshotId": snapshot,
        "room": room,
        "lines": lines,
        "approvals": approvals,
        "questions": questions,
        "working": team.working(&homes),
    }))
}

/// `GET /api/v1/mobile/room[?room=<projectId>]`
pub(super) async fn room(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<RoomQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let room = room_param(query.room);
    let value = tokio::task::spawn_blocking(move || {
        read_room(&db_path, room.as_deref(), query.after.as_deref())
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
