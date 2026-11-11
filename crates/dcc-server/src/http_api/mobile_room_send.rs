//! A message the phone writes in a room (`dcc_core::domain::team_room`):
//! sent once, to whoever the room's rule picks among its own lines (`@`, the
//! Reviewer's job, who answered last there, the Reviewer), in that agent's
//! conversation of the room — started the first time, as the desktop's room
//! does — on the agent's own provider, model, effort and conversation
//! approval (`dcc_core::domain::agent_run`). The answer comes in that
//! conversation; the phone reads the room again. While that conversation has
//! a turn open the message is queued for when it ends
//! (`dcc_core::domain::team_room_queue`), as the desktop's room does. A
//! message may also name its conversation (`sessionId`): "Tentar de novo"
//! on a turn that stopped, the same prompt to the same agent.

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
use dcc_core::domain::team_room_queue::must_wait;
use dcc_core::domain::workspace::WorkspaceState;
use dcc_core::ports::provider::ProviderRuntimeConfig;
use dcc_core::ports::WorkspaceRepo;
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use dcc_infra::git::CommandGitOps;
use dcc_tauri::commands::agent_conversation::agent_home_with_state;

use super::mobile_room::room_param;
use super::mobile_rooms::project_names;
use super::room_feed::{read_team, room_lines, Team};
use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct RoomMessage {
    text: String,
    /// The project whose room it is written in; none: Geral.
    #[serde(default)]
    room: Option<String>,
    /// The room's conversation it goes to, past the rule ("Tentar de novo").
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
    /// The stopped turn it tries again.
    #[serde(default, rename = "retryOf")]
    retry_of: Option<String>,
}

/// Where a message goes: the agent, why, its conversation in the room (none
/// until the first message) and the room's name.
pub(super) struct Destination {
    pub agent: ResidentAgent,
    pub reason: RouteReason,
    pub session_id: Option<String>,
    /// The agent's newest conversation in any room, for what it ran on.
    pub latest: Option<String>,
    pub room_name: Option<String>,
    /// That conversation has a turn open, or waits on the person: the
    /// message is queued.
    pub waits: bool,
}

/// What became of a message: the conversation it went to and, when it was
/// queued, the queued turn's id.
pub(super) struct Delivered {
    pub session_id: String,
    pub queued: Option<String>,
}

pub(super) fn reason_name(reason: RouteReason) -> &'static str {
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

/// The room's name, or the refusal when the project's room is gone.
pub(super) fn room_name(
    db_path: &Path,
    room: Option<&str>,
) -> Result<Result<Option<String>, HttpApiError>, String> {
    let Some(project) = room else {
        return Ok(Ok(None));
    };
    let names = project_names(db_path)?;
    Ok(match names.into_iter().find(|(id, _)| id == project) {
        Some((_, name)) => Ok(Some(name)),
        None => Err(HttpApiError::Refused(
            "roomNotFound",
            "this project's room is gone".to_string(),
            json!({ "room": project }),
        )),
    })
}

/// Where `agent` answers in `room`, read from `team`.
pub(super) fn destination_of(
    team: &Team,
    agent: &ResidentAgent,
    reason: RouteReason,
    session_id: Option<String>,
    room_name: Option<String>,
) -> Destination {
    let binding = session_id.as_deref().and_then(|id| {
        team.bindings
            .iter()
            .find(|binding| binding.session_id == id)
    });
    let waits = binding.is_some_and(|binding| {
        must_wait(
            team.running(binding),
            team.waiting(binding).is_some(),
            false,
        )
    });
    Destination {
        agent: agent.clone(),
        reason,
        latest: team.latest_conversation(&agent.id),
        session_id,
        room_name,
        waits,
    }
}

fn destination(
    db_path: &Path,
    text: &str,
    room: Option<&str>,
    session_id: Option<&str>,
) -> Result<Result<Destination, HttpApiError>, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let room_name = match room_name(db_path, room)? {
        Ok(name) => name,
        Err(refused) => return Ok(Err(refused)),
    };
    let team = read_team(&repo)?;
    if let Some(session_id) = session_id {
        // Only one of the room's own conversations.
        let agent = team
            .room_homes(room)
            .into_iter()
            .find(|home| home.session_id == session_id)
            .and_then(|home| team.agents.iter().find(|agent| agent.id == home.agent_id));
        return Ok(match agent {
            Some(agent) => Ok(destination_of(
                &team,
                agent,
                RouteReason::Continues,
                Some(session_id.to_string()),
                room_name,
            )),
            None => Err(HttpApiError::Refused(
                "roomNoConversation",
                "that conversation is not in this room".to_string(),
                json!({ "sessionId": session_id }),
            )),
        });
    }
    let lines = room_lines(&repo, &team.room_homes(room))?;
    let Some((agent, reason)) = pick(text, &team.agents, &lines, chrono::Utc::now()) else {
        return Ok(Err(HttpApiError::Refused(
            "roomNoAgent",
            "no agent can answer".to_string(),
            json!({}),
        )));
    };
    let session_id = team.conversation(&agent.id, room);
    Ok(Ok(destination_of(
        &team, agent, reason, session_id, room_name,
    )))
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

/// Sends `text` to `to` in `room`, on the agent's own provider and model:
/// into its conversation there (started the first time), or queued while
/// that conversation has a turn open. `retry_of`: the stopped turn it tries
/// again.
pub(super) async fn deliver(
    config: Arc<RwLock<HttpConfig>>,
    backend: &Arc<HttpBackend>,
    to: &Destination,
    room: Option<&str>,
    text: &str,
    retry_of: Option<&str>,
) -> Result<Delivered, HttpApiError> {
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
        None => start_conversation(backend, to, room, &pick, runtime.clone()).await?,
    };
    if retry_of.is_some() {
        // Not closed by the stop: nothing to reopen.
        let _ = super::resume_session_handler(
            State(backend.clone()),
            axum::extract::Path(session_id.clone()),
        )
        .await;
    }
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
        "retryOfTurnId": retry_of,
    }))
    .map_err(|error| HttpApiError::internal(error.to_string()))?;
    input.provider_runtime = runtime;
    mobile::resolve_turn_approval_policy(config, backend, &mut input).await?;
    if to.waits && to.session_id.is_some() {
        let queued = dcc_tauri::commands::session_commands::queue_turn_with_state(
            state,
            dcc_core::application::QueueTurnInput {
                turn: input.clone(),
            },
        )
        .await;
        match queued {
            Ok(queued) => {
                return Ok(Delivered {
                    session_id,
                    queued: Some(queued.id),
                })
            }
            // The turn ended meanwhile: it goes now.
            Err(error) if turn_ended(&error) => {}
            Err(error) => return Err(follow_up_error(error)),
        }
    }
    send_turn_with_state(state, input)
        .await
        .map_err(classify_session_error)?;
    Ok(Delivered {
        session_id,
        queued: None,
    })
}

/// Whether a queue was refused because no turn runs any more.
fn turn_ended(error: &str) -> bool {
    error.contains("only be queued while a turn is active")
}

/// What the phone reads back of a message sent: who took it and whether it
/// waits in the queue.
pub(super) fn sent_json(to: &Destination, delivered: &Delivered, room: Option<&str>) -> Value {
    json!({
        "agentId": to.agent.id,
        "agentName": to.agent.name,
        "reason": reason_name(to.reason),
        "sessionId": delivered.session_id,
        "room": room,
        "queued": delivered.queued.is_some(),
        "queuedId": delivered.queued,
    })
}

/// `POST /api/v1/mobile/room/messages`: `{text, room?}` to whoever the
/// rule picks in that room; with `sessionId` (and `retryOf`) to that
/// conversation of the room.
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
    let session_id = message
        .session_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    let to = {
        let (text, room) = (text.clone(), room.clone());
        tokio::task::spawn_blocking(move || {
            destination(&db_path, &text, room.as_deref(), session_id.as_deref())
        })
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::internal)??
    };
    let retry_of = message.retry_of.filter(|_| to.session_id.is_some());
    let delivered = deliver(
        config,
        &backend,
        &to,
        room.as_deref(),
        &text,
        retry_of.as_deref(),
    )
    .await?;
    Ok(Json(sent_json(&to, &delivered, room.as_deref())))
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

    #[test]
    fn a_message_to_a_conversation_at_work_waits_for_its_turn() {
        let team = super::super::room_feed::tests::team();
        let dev = &team.agents[0];
        // "s-dev-room" runs (and waits on a permission): queued.
        let busy = destination_of(
            &team,
            dev,
            RouteReason::Mention,
            Some("s-dev-room".into()),
            None,
        );
        assert!(busy.waits);
        let idle = destination_of(
            &team,
            dev,
            RouteReason::Mention,
            Some("s-geral".into()),
            None,
        );
        assert!(!idle.waits);
        let first = destination_of(&team, dev, RouteReason::Default, None, None);
        assert!(!first.waits);
        assert_eq!(first.latest.as_deref(), Some("s-dev-room"));
        let sent = sent_json(
            &busy,
            &Delivered {
                session_id: "s-dev-room".into(),
                queued: Some("q-1".into()),
            },
            Some("checkout"),
        );
        assert_eq!(sent["queued"], true);
        assert_eq!(sent["queuedId"], "q-1");
        assert_eq!(sent["reason"], "mention");
        assert!(turn_ended(
            "follow-ups can only be queued while a turn is active"
        ));
        assert!(!turn_ended("session not found"));
    }
}
