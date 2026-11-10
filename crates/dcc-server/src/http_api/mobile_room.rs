//! The team room on the phone (`dcc_core::domain::team_room`): the latest
//! lines of every agent's own conversation together in time, read from
//! DCC's database like the desktop's room, and a message the phone writes
//! once, sent to whoever the room's rule picks (`@`, the Reviewer's job,
//! who answered last, the Reviewer). The answer comes in the agent's own
//! conversation; the phone reads the room again. A phone that sends back
//! the `snapshotId` it holds as `after` gets `{unchanged, snapshotId}`
//! without a single event read while no conversation moved.

use std::collections::HashMap;
use std::path::Path;

use dcc_core::application::SendTurnInput;
use dcc_core::domain::agent::{ResidentAgent, ReviewerPresetText};
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::session::SessionId;
use dcc_core::domain::team_room::{route, RouteReason, Teammate};
use dcc_core::domain::team_room_lines::{last_answer, lines_of, RoomLine};
use dcc_core::domain::timeline::activity::total_turn_tokens;
use dcc_core::domain::timeline::project_session;
use dcc_core::ports::UsageRepo;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

use super::*;

/// How many of the latest lines the phone gets.
const ROOM_LINES: usize = 60;
const EVENT_PAGE: usize = 500;

#[derive(Debug, Default, Deserialize)]
pub(super) struct RoomQuery {
    /// The `snapshotId` the phone already holds.
    after: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RoomMessage {
    text: String,
}

/// The agents and each one's own conversations, newest last.
struct Homes {
    agents: Vec<ResidentAgent>,
    homes: Vec<AgentSessionBinding>,
}

fn homes(repo: &SqliteSessionRepo) -> Result<Homes, String> {
    let agents = repo
        .list_stored_resident_agents(&ReviewerPresetText::default())
        .map_err(|error| error.to_string())?;
    let mut homes: Vec<AgentSessionBinding> = repo
        .list_agent_session_bindings()
        .map_err(|error| error.to_string())?
        .into_iter()
        // The phone shows the Geral room for now.
        .filter(|binding| {
            binding.project_id == home_project_id(&binding.agent_id) && binding.room.is_none()
        })
        .filter(|binding| agents.iter().any(|agent| agent.id == binding.agent_id))
        .collect();
    homes.sort_by(|left, right| left.updated_at.cmp(&right.updated_at));
    Ok(Homes { agents, homes })
}

/// What changes when a conversation moves or the team changes.
fn snapshot_id(homes: &Homes) -> String {
    let mut digest = Sha256::new();
    for agent in &homes.agents {
        digest.update(agent.id.as_bytes());
        digest.update(agent.name.as_bytes());
    }
    for home in &homes.homes {
        digest.update(home.session_id.as_bytes());
        digest.update(home.updated_at.as_bytes());
    }
    digest.finalize()[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One conversation's lines, from all its events.
fn conversation_lines(
    repo: &SqliteSessionRepo,
    agent_id: &str,
    session_id: &str,
) -> Result<Vec<RoomLine>, String> {
    let id = SessionId(session_id.to_string());
    let mut events = Vec::new();
    let mut cursor = 0;
    loop {
        let page = repo
            .list_events_by_session_after_sync(&id, cursor, EVENT_PAGE)
            .map_err(|error| error.to_string())?;
        let full = page.len() == EVENT_PAGE;
        if let Some(last) = page.last() {
            cursor = last.sequence;
        }
        events.extend(page);
        if !full {
            break;
        }
    }
    let tokens: HashMap<String, u64> =
        // On a blocking thread of the runtime: waiting here is fine.
        tokio::runtime::Handle::current()
            .block_on(repo.list_session_turn_usage(&id))
            .map_err(|error| error.to_string())?
            .iter()
            .filter_map(|turn| {
                total_turn_tokens(&turn.models).map(|totals| (turn.turn_id.clone(), totals.total))
            })
            .collect();
    let messages = project_session(&events, session_id);
    Ok(lines_of(agent_id, session_id, &messages, &tokens))
}

/// Every conversation's lines in time order, the latest `ROOM_LINES`.
fn room_lines(repo: &SqliteSessionRepo, homes: &Homes) -> Result<Vec<RoomLine>, String> {
    let mut lines = Vec::new();
    for home in &homes.homes {
        lines.extend(conversation_lines(repo, &home.agent_id, &home.session_id)?);
    }
    lines.sort_by(|left, right| left.at.cmp(&right.at));
    let skip = lines.len().saturating_sub(ROOM_LINES);
    Ok(lines.split_off(skip))
}

fn read_room(db_path: &Path, after: Option<&str>) -> Result<Value, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let homes = homes(&repo)?;
    let snapshot = snapshot_id(&homes);
    if after == Some(snapshot.as_str()) {
        return Ok(json!({ "unchanged": true, "snapshotId": snapshot }));
    }
    let lines = room_lines(&repo, &homes)?;
    Ok(json!({ "snapshotId": snapshot, "lines": lines }))
}

/// `GET /api/v1/mobile/room`
pub(super) async fn room(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<RoomQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let value = tokio::task::spawn_blocking(move || read_room(&db_path, query.after.as_deref()))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::internal)?;
    Ok(Json(value))
}

/// Where a message goes: the agent, why, and its own conversation (none
/// until the desktop started it).
struct Destination {
    agent: ResidentAgent,
    reason: RouteReason,
    session_id: Option<String>,
}

fn destination(db_path: &Path, text: &str) -> Result<Option<Destination>, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let homes = homes(&repo)?;
    let lines = room_lines(&repo, &homes)?;
    let lines: Vec<&RoomLine> = lines.iter().collect();
    let last = last_answer(&lines, chrono::Utc::now());
    let team: Vec<Teammate<'_>> = homes
        .agents
        .iter()
        .map(|agent| Teammate {
            id: &agent.id,
            name: &agent.name,
            preset: agent.preset.as_deref(),
        })
        .collect();
    let Some((mate, reason)) = route(
        text,
        &team,
        last.as_ref().map(|(id, minutes)| (id.as_str(), *minutes)),
    ) else {
        return Ok(None);
    };
    let Some(agent) = homes
        .agents
        .iter()
        .find(|agent| agent.id == mate.id)
        .cloned()
    else {
        return Ok(None);
    };
    let session_id = homes
        .homes
        .iter()
        .rev()
        .find(|home| home.agent_id == agent.id)
        .map(|home| home.session_id.clone());
    Ok(Some(Destination {
        agent,
        reason,
        session_id,
    }))
}

/// `POST /api/v1/mobile/room/messages`: `{text}` to whoever the rule picks.
pub(super) async fn send(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Json(message): Json<RoomMessage>,
) -> Result<Json<Value>, HttpApiError> {
    let text = message.text.trim().to_string();
    if text.is_empty() {
        return Err(HttpApiError::bad_request("empty message".to_string()));
    }
    let db_path = backend.workspace.db_path.clone();
    let routed = {
        let text = text.clone();
        tokio::task::spawn_blocking(move || destination(&db_path, &text))
            .await
            .map_err(|error| HttpApiError::internal(error.to_string()))?
            .map_err(HttpApiError::internal)?
    };
    let Some(to) = routed else {
        return Err(HttpApiError::Refused(
            "roomNoAgent",
            "no agent can answer".to_string(),
            json!({}),
        ));
    };
    let Some(session_id) = to.session_id.clone() else {
        return Err(HttpApiError::Refused(
            "roomNoConversation",
            format!(
                "{} has no conversation yet: start it in the desktop's team room",
                to.agent.name
            ),
            json!({ "agentName": to.agent.name }),
        ));
    };
    let mut input: SendTurnInput = serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": text,
    }))
    .map_err(|error| HttpApiError::internal(error.to_string()))?;
    mobile::resolve_turn_approval_policy(config, &backend, &mut input).await?;
    send_turn_with_state(&backend.sessions, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Json(json!({
        "agentId": to.agent.id,
        "agentName": to.agent.name,
        "reason": match to.reason {
            RouteReason::Mention => "mention",
            RouteReason::Role => "role",
            RouteReason::Continues => "continues",
            RouteReason::Default => "default",
        },
        "sessionId": session_id,
    })))
}
