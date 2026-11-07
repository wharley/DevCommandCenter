//! The phone's "Agentes": each resident agent that talks (the Reporter only
//! writes the desktop's recap), with its mascot, how it is and its latest
//! news, and its own conversation to open. Read from DCC's database like the
//! desktop's strip; whether a trigger is on comes from the desktop's own
//! `agent-triggers.json` beside it. Like the attention snapshot, a phone
//! that sends back the `snapshotId` it holds as `after` gets
//! `{unchanged, snapshotId}` while nothing changed.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use dcc_core::domain::agent::{ResidentAgent, ReviewerPresetText, CHRONICLER_PRESET};
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::attention::SessionActivityInput;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

use super::*;

#[derive(Debug, Default, Deserialize)]
pub(super) struct AgentsQuery {
    /// The `snapshotId` the phone already holds.
    after: Option<String>,
}

/// A session's activity and when its last turn finished.
pub(super) struct Activity {
    pub input: SessionActivityInput,
    pub completed_at: Option<String>,
}

/// The agents with a trigger on, from the desktop's file; none when it is
/// missing or unreadable.
fn watching(app_data: &Path) -> HashSet<String> {
    std::fs::read_to_string(app_data.join("agent-triggers.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|stored| stored.get("on").and_then(Value::as_object).cloned())
        .map(|on| {
            on.into_iter()
                .filter(|(_, triggers)| triggers.as_array().is_some_and(|list| !list.is_empty()))
                .map(|(agent, _)| agent)
                .collect()
        })
        .unwrap_or_default()
}

/// What the phone shows, newest-news aside: `needsYou`, `working`,
/// `watching` or `asleep`.
pub(super) fn agents_view(
    agents: &[ResidentAgent],
    bindings: &[AgentSessionBinding],
    activity: &HashMap<String, Activity>,
    watching: &HashSet<String>,
) -> Value {
    let list: Vec<Value> = agents
        .iter()
        .filter(|agent| agent.preset.as_deref() != Some(CHRONICLER_PRESET))
        .map(|agent| {
            let sessions: Vec<&AgentSessionBinding> = bindings
                .iter()
                .filter(|binding| binding.agent_id == agent.id)
                .collect();
            let running = |waiting: bool| {
                sessions.iter().any(|binding| {
                    activity.get(&binding.session_id).is_some_and(|activity| {
                        activity.input.is_running()
                            && activity.input.waiting_reason().is_some() == waiting
                    })
                })
            };
            let state = if running(true) {
                "needsYou"
            } else if running(false) {
                "working"
            } else if watching.contains(&agent.id) {
                "watching"
            } else {
                "asleep"
            };
            let home = home_project_id(&agent.id);
            let home_session = sessions
                .iter()
                .filter(|binding| binding.project_id == home)
                .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
                .map(|binding| binding.session_id.clone());
            let last = sessions
                .iter()
                .filter_map(|binding| {
                    let at = activity.get(&binding.session_id)?.completed_at.clone()?;
                    Some((at, *binding))
                })
                .max_by(|left, right| left.0.cmp(&right.0))
                .map(|(at, binding)| {
                    json!({
                        "sessionId": binding.session_id,
                        "title": binding.title,
                        "at": at,
                        "inConversation": binding.project_id == home,
                    })
                });
            json!({
                "id": agent.id,
                "name": agent.name,
                "preset": agent.preset,
                "avatar": {
                    "color": agent.avatar.color,
                    "arms": agent.avatar.arms,
                    "eyes": agent.avatar.eyes,
                },
                "state": state,
                "conversationSessionId": home_session,
                "last": last,
            })
        })
        .collect();
    json!({ "agents": list })
}

/// The view from DCC's database and the desktop's trigger file.
fn read(db_path: &Path, app_data: &Path) -> Result<Value, String> {
    let repo = SqliteSessionRepo::open_read_only(db_path).map_err(|error| error.to_string())?;
    let agents = repo
        .list_stored_resident_agents(&ReviewerPresetText::default())
        .map_err(|error| error.to_string())?;
    let bindings = repo
        .list_agent_session_bindings()
        .map_err(|error| error.to_string())?;
    let mut workspaces: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.workspace_id.as_str())
        .collect();
    workspaces.sort_unstable();
    workspaces.dedup();
    let mut activity = HashMap::new();
    for workspace in workspaces {
        let summaries = repo
            .list_workspace_sessions(&WorkspaceId(workspace.to_string()))
            .map_err(|error| error.to_string())?;
        for summary in summaries {
            activity.insert(
                summary.session.id.0.clone(),
                Activity {
                    input: SessionActivityInput::from_summary(&summary),
                    completed_at: summary.last_turn_completed_at.clone(),
                },
            );
        }
    }
    Ok(agents_view(
        &agents,
        &bindings,
        &activity,
        &watching(app_data),
    ))
}

pub(super) async fn agents(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<AgentsQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let db_path = backend.workspace.db_path.clone();
    let app_data = db_path.parent().map(Path::to_path_buf).unwrap_or_default();
    let value = tokio::task::spawn_blocking(move || read(&db_path, &app_data))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::internal)?;
    Ok(Json(attention::conditional_snapshot(
        value,
        query.after.as_deref(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::AgentAvatar;

    fn agent(id: &str, preset: &str) -> ResidentAgent {
        ResidentAgent {
            id: id.into(),
            name: id.into(),
            role: String::new(),
            kickoff_prompt: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            provider_id: None,
            model: None,
            effort: None,
            avatar: AgentAvatar {
                color: "violet".into(),
                arms: 3,
                eyes: "round".into(),
            },
            preset: Some(preset.into()),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn binding(session: &str, agent: &str, project: &str, at: &str) -> AgentSessionBinding {
        AgentSessionBinding {
            session_id: session.into(),
            agent_id: agent.into(),
            workspace_id: format!("w-{session}"),
            project_id: project.into(),
            title: Some(format!("Tarefa {session}")),
            updated_at: at.into(),
        }
    }

    fn activity(running: bool, waiting: bool, completed: Option<&str>) -> Activity {
        Activity {
            input: SessionActivityInput {
                session_updated_at: String::new(),
                projection_state: if running { "active" } else { "completed" }.into(),
                active_turn_id: running.then(|| "t".to_string()),
                projection_updated_at: None,
                turn_count: 1,
                last_turn_state: Some(if running { "running" } else { "completed" }.into()),
                last_turn_started_at: None,
                last_turn_completed_at: completed.map(str::to_string),
                last_turn_awaiting_user: waiting.then(|| "permission".to_string()),
                last_turn_awaiting_since: None,
                usage_limit: None,
            },
            completed_at: completed.map(str::to_string),
        }
    }

    #[test]
    fn each_talking_agent_with_its_state_conversation_and_news() {
        let agents = [
            agent("rev", "reviewer"),
            agent("res", "researcher"),
            agent("rep", "chronicler"),
        ];
        let bindings = [
            binding("s-task", "rev", "p-loja", "2026-11-07T10:00:00Z"),
            binding("s-home-old", "rev", "dcc-agent-rev", "2026-11-06T10:00:00Z"),
            binding("s-home", "rev", "dcc-agent-rev", "2026-11-07T09:00:00Z"),
            binding("s-idea", "res", "p-ideia", "2026-11-07T08:00:00Z"),
        ];
        let activity = HashMap::from([
            (
                "s-task".to_string(),
                activity(false, false, Some("2026-11-07T10:05:00Z")),
            ),
            (
                "s-home".to_string(),
                activity(false, false, Some("2026-11-07T09:30:00Z")),
            ),
            ("s-idea".to_string(), activity(true, true, None)),
        ]);
        let watching = HashSet::from(["rev".to_string()]);
        let view = agents_view(&agents, &bindings, &activity, &watching);
        let list = view["agents"].as_array().unwrap();
        assert_eq!(list.len(), 2, "the Reporter does not talk");
        assert_eq!(list[0]["state"], "watching");
        assert_eq!(list[0]["conversationSessionId"], "s-home");
        assert_eq!(list[0]["last"]["sessionId"], "s-task");
        assert_eq!(list[0]["last"]["inConversation"], false);
        assert_eq!(list[0]["avatar"]["arms"], 3);
        assert_eq!(list[1]["state"], "needsYou");
        assert_eq!(list[1]["conversationSessionId"], Value::Null);
        assert_eq!(list[1]["last"], Value::Null);

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("agent-triggers.json"),
            r#"{"on":{"rev":["prUpdated"],"off":[]},"runs":[]}"#,
        )
        .unwrap();
        assert_eq!(watching_for(dir.path()), HashSet::from(["rev".to_string()]));
        assert!(watching_for(&dir.path().join("missing")).is_empty());
    }

    fn watching_for(path: &Path) -> HashSet<String> {
        watching(path)
    }
}
