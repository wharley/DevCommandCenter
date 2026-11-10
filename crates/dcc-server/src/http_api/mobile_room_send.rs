//! A message the phone writes in a room (`dcc_core::domain::team_room`):
//! sent once, to whoever the room's rule picks among its own lines (`@`, the
//! Reviewer's job, who answered last there, the Reviewer), in that agent's
//! conversation of the room — started the first time, as the desktop's room
//! does — on the agent's own provider, model, effort and conversation
//! approval (`dcc_core::domain::agent_run`). The answer comes in that
//! conversation; the phone reads the room again.

use std::path::Path;

use dcc_core::application::{
    create_workspace_for_repo, CreateWorkspaceForRepoInput, WorkspaceIsolationMode,
};
use dcc_core::domain::agent::ResidentAgent;
use dcc_core::domain::agent_run::{
    agent_run_effort, agent_run_pick, agent_run_runtime, AgentRunError, AgentRunPick,
};
use dcc_core::domain::project::ProjectId;
use dcc_core::domain::provider::{HealthStatus, ProviderApprovalPolicy, ProviderDescriptor};
use dcc_core::domain::team_room::{route, RouteReason, Teammate};
use dcc_core::domain::team_room_lines::{last_answer, RoomLine};
use dcc_core::domain::workspace::WorkspaceState;
use dcc_core::ports::provider::ProviderRuntimeConfig;
use dcc_core::ports::WorkspaceRepo;
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use dcc_infra::git::CommandGitOps;
use dcc_tauri::commands::agent_conversation::agent_home_with_state;

use super::mobile_room::room_param;
use super::mobile_rooms::project_names;
use super::room_feed::{read_team, room_lines};
use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct RoomMessage {
    text: String,
    /// The project whose room it is written in; none: Geral.
    #[serde(default)]
    room: Option<String>,
}

/// Where a message goes: the agent, why, its conversation in the room (none
/// until the first message) and the room's name.
struct Destination {
    agent: ResidentAgent,
    reason: RouteReason,
    session_id: Option<String>,
    /// The agent's newest conversation in any room, for what it ran on.
    latest: Option<String>,
    room_name: Option<String>,
}

fn reason_name(reason: RouteReason) -> &'static str {
    match reason {
        RouteReason::Mention => "mention",
        RouteReason::Role => "role",
        RouteReason::Continues => "continues",
        RouteReason::Default => "default",
    }
}

/// Picks among `agents` by the room's own `lines`.
fn pick<'a>(
    text: &str,
    agents: &'a [ResidentAgent],
    lines: &[RoomLine],
    now: chrono::DateTime<chrono::Utc>,
) -> Option<(&'a ResidentAgent, RouteReason)> {
    let lines: Vec<&RoomLine> = lines.iter().collect();
    let last = last_answer(&lines, now);
    let team: Vec<Teammate<'_>> = agents
        .iter()
        .map(|agent| Teammate {
            id: &agent.id,
            name: &agent.name,
            preset: agent.preset.as_deref(),
        })
        .collect();
    let (mate, reason) = route(
        text,
        &team,
        last.as_ref().map(|(id, minutes)| (id.as_str(), *minutes)),
    )?;
    let agent = agents.iter().find(|agent| agent.id == mate.id)?;
    Some((agent, reason))
}

fn destination(
    db_path: &Path,
    text: &str,
    room: Option<&str>,
) -> Result<Result<Destination, HttpApiError>, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let room_name = match room {
        None => None,
        Some(project) => {
            let names = project_names(db_path)?;
            let Some((_, name)) = names.into_iter().find(|(id, _)| id == project) else {
                return Ok(Err(HttpApiError::Refused(
                    "roomNotFound",
                    "this project's room is gone".to_string(),
                    json!({ "room": project }),
                )));
            };
            Some(name)
        }
    };
    let team = read_team(&repo)?;
    let lines = room_lines(&repo, &team.room_homes(room))?;
    let Some((agent, reason)) = pick(text, &team.agents, &lines, chrono::Utc::now()) else {
        return Ok(Err(HttpApiError::Refused(
            "roomNoAgent",
            "no agent can answer".to_string(),
            json!({}),
        )));
    };
    Ok(Ok(Destination {
        agent: agent.clone(),
        reason,
        session_id: team.conversation(&agent.id, room),
        latest: team.latest_conversation(&agent.id),
        room_name,
    }))
}

/// The provider and model picked "where the message is written": on the
/// phone, what the agent ran on last, else the first provider that can run.
fn fallback_pick(
    last: Option<(String, Option<String>)>,
    providers: &[ProviderDescriptor],
) -> (Option<String>, Option<String>) {
    if let Some(last) = last {
        return (Some(last.0), last.1);
    }
    let Some(provider) = providers.iter().find(|provider| {
        provider.enabled && !matches!(provider.health, HealthStatus::Unhealthy { .. })
    }) else {
        return (None, None);
    };
    let model = provider
        .models
        .iter()
        .find(|model| model.recommended)
        .or_else(|| provider.models.first())
        .map(|model| model.id.clone());
    (Some(provider.id.0.clone()), model)
}

/// "Conversa com Dev", "· Loja" in a project's room, as the desktop names it.
fn conversation_title(agent: &str, room_name: Option<&str>) -> String {
    match room_name {
        Some(project) => format!("Conversa com {agent} · {project}"),
        None => format!("Conversa com {agent}"),
    }
}

/// The agent's own conversation in `room`, started: its home task (made
/// the first time), a session there on `pick`, bound to the agent and room.
async fn start_conversation(
    backend: &HttpBackend,
    to: &Destination,
    room: Option<&str>,
    pick: &AgentRunPick,
    runtime: Option<ProviderRuntimeConfig>,
) -> Result<String, HttpApiError> {
    let state = &backend.sessions;
    let home = agent_home_with_state(state, &to.agent.id)
        .await
        .map_err(HttpApiError::bad_request)?;
    let title = conversation_title(&to.agent.name, to.room_name.as_deref());
    let repo = SqliteWorkspaceRepo::open(&backend.workspace.db_path)
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    let open_home = repo
        .list_workspaces()
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .into_iter()
        .filter(|workspace| workspace.project_id.0 == home.project_id)
        .find(|workspace| {
            !matches!(
                workspace.state,
                WorkspaceState::Archived | WorkspaceState::Completed
            )
        });
    let workspace = match open_home {
        Some(workspace) => workspace,
        None => {
            create_workspace_for_repo(
                &repo,
                &CommandGitOps::new(),
                state,
                CreateWorkspaceForRepoInput {
                    project_id: ProjectId(home.project_id.clone()),
                    workspace_root: home.root_path.clone(),
                    base_branch: "main".into(),
                    name: Some(title),
                    isolation_mode: Some(WorkspaceIsolationMode::LocalDirect),
                    context_folders: Vec::new(),
                },
            )
            .await
            .map_err(|error| classify_session_error(error.to_string()))?
            .workspace
        }
    };
    let mut start: StartThreadInput = serde_json::from_value(json!({
        "workspaceId": workspace.id,
        "projectId": home.project_id,
        "providerId": pick.provider_id,
        "model": pick.model,
        "title": to.agent.name,
    }))
    .map_err(|error| HttpApiError::internal(error.to_string()))?;
    start.provider_runtime = runtime;
    let session = start_thread_with_state(state, start)
        .await
        .map_err(classify_session_error)?
        .session
        .id;
    let bound = state
        .bind_session_agent(&session, &to.agent.id)
        .and_then(|()| match room {
            Some(project) => state.set_session_room(&session, project),
            None => Ok(()),
        });
    if let Err(error) = bound {
        // Without the binding it would be a plain session with no role.
        let _ = close_session(
            state,
            state,
            state,
            CloseSessionInput {
                session_id: session.clone(),
                delete_history: true,
            },
        )
        .await;
        return Err(HttpApiError::internal(error.to_string()));
    }
    Ok(session.0)
}

fn run_error(agent: &str, error: AgentRunError) -> HttpApiError {
    match error {
        AgentRunError::Unavailable => HttpApiError::Refused(
            "agentProviderUnavailable",
            format!("{agent}'s provider is not available"),
            json!({ "agentName": agent }),
        ),
        AgentRunError::Unhealthy(reason) => HttpApiError::Refused(
            "agentProviderUnhealthy",
            reason,
            json!({ "agentName": agent }),
        ),
    }
}

/// `POST /api/v1/mobile/room/messages`: `{text, room?}` to whoever the
/// rule picks in that room.
pub(super) async fn send(
    State(config): State<Arc<RwLock<HttpConfig>>>,
    State(backend): State<Arc<HttpBackend>>,
    Json(message): Json<RoomMessage>,
) -> Result<Json<Value>, HttpApiError> {
    let text = message.text.trim().to_string();
    if text.is_empty() {
        return Err(HttpApiError::bad_request("empty message".to_string()));
    }
    let room = room_param(message.room);
    let db_path = backend.workspace.db_path.clone();
    let to = {
        let (text, room) = (text.clone(), room.clone());
        tokio::task::spawn_blocking(move || destination(&db_path, &text, room.as_deref()))
            .await
            .map_err(|error| HttpApiError::internal(error.to_string()))?
            .map_err(HttpApiError::internal)??
    };
    let state = &backend.sessions;
    let peek = |id: Option<String>| async move {
        match id {
            Some(id) => state.peek_session(&SessionId(id)).await.ok().flatten(),
            None => None,
        }
    };
    let current = peek(to.session_id.clone()).await;
    let latest = peek(to.latest.clone()).await;
    let providers = dcc_tauri::commands::provider_commands::list_providers_for_state(state)
        .await
        .map_err(HttpApiError::internal)?
        .catalog
        .providers;
    let picked = fallback_pick(
        latest
            .as_ref()
            .map(|session| (session.provider_id.clone(), session.model.clone())),
        &providers,
    );
    let pick = agent_run_pick(
        to.agent.provider_id.as_deref(),
        to.agent.model.as_deref(),
        &providers,
        (picked.0.as_deref(), picked.1.as_deref()),
    )
    .map_err(|error| run_error(&to.agent.name, error))?;
    // Its own account on its own provider.
    let own_account = (to.agent.provider_id.as_deref() == Some(pick.provider_id.as_str())
        && providers.iter().any(|provider| {
            provider.id.0 == pick.provider_id && provider.capabilities.supports_accounts
        }))
    .then(|| to.agent.account_id.clone());
    // What it ran with on that provider: this conversation's, else its latest.
    let base = [current.as_ref(), latest.as_ref()]
        .into_iter()
        .flatten()
        .find(|session| session.provider_id == pick.provider_id)
        .map(|session| session.provider_runtime.clone());
    // On its provider a conversation keeps its runtime and the account it
    // started with (`send_turn`); a new one, or one moving to another
    // provider, gets a whole runtime so no other provider's path stays.
    let keeps = current
        .as_ref()
        .is_some_and(|session| session.provider_id == pick.provider_id);
    let runtime =
        (!keeps).then(|| agent_run_runtime(base.flatten(), own_account).unwrap_or_default());
    let session_id = match to.session_id.clone() {
        Some(session) => session,
        None => start_conversation(&backend, &to, room.as_deref(), &pick, runtime.clone()).await?,
    };
    let mut input: SendTurnInput = serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": text,
        "providerId": pick.provider_id,
        "model": pick.model,
        "effort": agent_run_effort(to.agent.provider_id.as_deref(), to.agent.effort.as_deref()),
        "planMode": false,
        "fastMode": false,
        "approvalPolicy": to
            .agent
            .tool_access
            .conversation_approval
            .unwrap_or(ProviderApprovalPolicy::Ask),
    }))
    .map_err(|error| HttpApiError::internal(error.to_string()))?;
    input.provider_runtime = runtime;
    mobile::resolve_turn_approval_policy(config, &backend, &mut input).await?;
    send_turn_with_state(state, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Json(json!({
        "agentId": to.agent.id,
        "agentName": to.agent.name,
        "reason": reason_name(to.reason),
        "sessionId": session_id,
        "room": room,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::team_room_lines::Speaker;

    fn answered(agent: &str, at: &str) -> RoomLine {
        RoomLine {
            key: format!("{agent}:{at}"),
            agent_id: agent.into(),
            session_id: format!("s-{agent}"),
            speaker: Speaker::Agent,
            text: "ok".into(),
            at: at.into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        }
    }

    #[test]
    fn the_rooms_own_lines_decide_who_continues() {
        let agents = vec![
            super::super::room_feed::tests::agent("dev"),
            super::super::room_feed::tests::agent("ops"),
        ];
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-10T10:05:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let lines = [answered("ops", "2026-11-10T10:00:00Z")];
        let (agent, reason) = pick("e o deploy?", &agents, &lines, now).unwrap();
        assert_eq!((agent.id.as_str(), reason), ("ops", RouteReason::Continues));
        let (agent, reason) = pick("@Agente dev olha isso", &agents, &lines, now).unwrap();
        assert_eq!((agent.id.as_str(), reason), ("dev", RouteReason::Mention));
        assert_eq!(reason_name(RouteReason::Default), "default");
    }

    #[test]
    fn a_conversation_is_named_after_the_agent_and_its_room() {
        assert_eq!(conversation_title("Dev", None), "Conversa com Dev");
        assert_eq!(
            conversation_title("Dev", Some("Loja")),
            "Conversa com Dev · Loja"
        );
    }

    #[test]
    fn with_nothing_run_before_the_first_provider_that_can_run_is_picked() {
        let provider = |id: &str, health: Value| -> ProviderDescriptor {
            serde_json::from_value(json!({
                "id": id, "label": id, "description": "",
                "models": [
                    { "id": "a", "label": "a", "description": "", "recommended": false, "effortLevels": [] },
                    { "id": "b", "label": "b", "description": "", "recommended": true, "effortLevels": [] }
                ],
                "capabilities": {
                    "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
                    "resumable": true, "experimental": false, "canBeDelegationTarget": true,
                    "canRequestDelegation": true, "supportsReadOnlyDelegation": true, "supportsEditDelegation": true
                },
                "health": health, "stable": true
            }))
            .unwrap()
        };
        let providers = [
            provider("cursor", json!({ "Unhealthy": { "reason": "x" } })),
            provider("codex", json!("Healthy")),
        ];
        assert_eq!(
            fallback_pick(None, &providers),
            (Some("codex".into()), Some("b".into()))
        );
        assert_eq!(
            fallback_pick(Some(("claude_code".into(), None)), &providers),
            (Some("claude_code".into()), None)
        );
        assert_eq!(fallback_pick(None, &[]), (None, None));
    }
}
