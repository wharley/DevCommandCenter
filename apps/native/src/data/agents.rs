//! Resident agents and what each is doing (`use-agents.ts`,
//! `agent-activity.ts`): the state of every session bound to an agent, folded
//! into one state for its avatar.

use std::collections::HashMap;

use dcc_core::domain::agent::{AgentAvatar, ReviewerPresetText};
use dcc_core::domain::attention::SessionActivityInput;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::db::SqliteSessionRepo;

pub const CHRONICLER_PRESET: &str = "chronicler";

/// What the agent is doing, shown on its mascot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    NeedsYou,
    Done,
}

impl AgentState {
    /// `agents.state.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Idle => "agents.state.idle",
            Self::Working => "agents.state.working",
            Self::NeedsYou => "agents.state.needsYou",
            Self::Done => "agents.state.done",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentSession {
    pub session_id: String,
    /// The task it runs in.
    pub workspace_id: String,
    pub state: AgentState,
    /// When its last turn finished, if it did.
    pub completed_at: Option<String>,
}

/// An agent as stored, with its sessions' live state; whether a finished
/// result is unread is the sidebar's reading marker.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub preset: Option<String>,
    pub avatar: AgentAvatar,
    /// Its own provider, model and effort; none runs on the composer's.
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// What a call asks it first.
    pub kickoff_prompt: String,
    pub sessions: Vec<AgentSession>,
}

/// The agent a session runs as, for its mascot (`SessionAgentBadge`).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionAgent {
    pub name: String,
    pub avatar: AgentAvatar,
    /// That session's own state.
    pub state: AgentState,
}

/// Every agent's sessions, by session id (`agentBySessionId`).
pub fn by_session(agents: &[AgentRecord]) -> HashMap<String, SessionAgent> {
    agents
        .iter()
        .flat_map(|agent| {
            agent.sessions.iter().map(move |session| {
                (
                    session.session_id.clone(),
                    SessionAgent {
                        name: agent.name.clone(),
                        avatar: agent.avatar.clone(),
                        state: session.state,
                    },
                )
            })
        })
        .collect()
}

/// `loadLatestReview`: the findings of the latest review a reviewer's
/// conversation holds; `None` before its first. A later answer without the
/// block (a question to the reviewer) does not replace the review before it.
pub fn latest_review(
    db_path: &std::path::Path,
    session_id: &str,
) -> Option<Vec<dcc_core::domain::review_findings::ReviewFinding>> {
    use dcc_core::domain::session::AssistantMessagePhase;
    use dcc_core::domain::timeline::{Role, project_session};
    let events = super::conversation::session_events(db_path, session_id).ok()?;
    project_session(&events, session_id)
        .iter()
        .rev()
        .filter(|message| {
            message.role == Role::Assistant
                && message.assistant_phase != Some(AssistantMessagePhase::Commentary)
        })
        .find_map(|message| {
            dcc_core::domain::review_findings::parse_review_findings(&message.content)
        })
}

/// `agentSessionState`
pub fn session_state(input: Option<&SessionActivityInput>) -> AgentState {
    let Some(input) = input else {
        return AgentState::Idle;
    };
    if input.is_running() {
        return if input.waiting_reason().is_some() {
            AgentState::NeedsYou
        } else {
            AgentState::Working
        };
    }
    if input.last_turn_state.as_deref() == Some("completed") {
        AgentState::Done
    } else {
        AgentState::Idle
    }
}

/// `aggregateAgentState`: most urgent first; a finished session counts only
/// while its result is unread.
pub fn aggregate(sessions: &[(AgentState, bool)]) -> AgentState {
    if sessions
        .iter()
        .any(|(state, _)| *state == AgentState::NeedsYou)
    {
        return AgentState::NeedsYou;
    }
    if sessions
        .iter()
        .any(|(state, _)| *state == AgentState::Working)
    {
        return AgentState::Working;
    }
    if sessions
        .iter()
        .any(|(state, unread)| *state == AgentState::Done && *unread)
    {
        AgentState::Done
    } else {
        AgentState::Idle
    }
}

/// `isResultUnread`
pub fn is_result_unread(completed_at: Option<&str>, seen_at: Option<&str>) -> bool {
    match (completed_at, seen_at) {
        (Some(completed), Some(seen)) => seen < completed,
        (Some(_), None) => true,
        _ => false,
    }
}

/// The agent that speaks for the row: the one that needs the person, else a
/// result not opened yet, else one at work.
pub fn lead<T>(agents: &[T], state: impl Fn(&T) -> AgentState) -> Option<&T> {
    [AgentState::NeedsYou, AgentState::Done, AgentState::Working]
        .into_iter()
        .find_map(|wanted| agents.iter().find(|agent| state(agent) == wanted))
}

/// The agents and their sessions' state, read only. The built-in agents are
/// seeded by the desktop.
pub fn load(sessions: &SqliteSessionRepo) -> anyhow::Result<Vec<AgentRecord>> {
    let agents = sessions.list_stored_resident_agents(&ReviewerPresetText::default())?;
    let bindings = sessions.list_agent_session_bindings()?;
    let mut inputs: HashMap<String, (SessionActivityInput, Option<String>)> = HashMap::new();
    let mut workspaces: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.workspace_id.as_str())
        .collect();
    workspaces.sort_unstable();
    workspaces.dedup();
    for workspace_id in workspaces {
        for summary in sessions.list_workspace_sessions(&WorkspaceId(workspace_id.to_string()))? {
            let completed = summary.last_turn_completed_at.clone();
            inputs.insert(
                summary.session.id.0.clone(),
                (SessionActivityInput::from_summary(&summary), completed),
            );
        }
    }
    Ok(agents
        .into_iter()
        .map(|agent| {
            let sessions = bindings
                .iter()
                .filter(|binding| binding.agent_id == agent.id)
                .map(|binding| {
                    let input = inputs.get(&binding.session_id);
                    AgentSession {
                        session_id: binding.session_id.clone(),
                        workspace_id: binding.workspace_id.clone(),
                        state: session_state(input.map(|(input, _)| input)),
                        completed_at: input.and_then(|(_, completed)| completed.clone()),
                    }
                })
                .collect();
            AgentRecord {
                id: agent.id,
                name: agent.name,
                preset: agent.preset,
                avatar: agent.avatar,
                provider_id: agent.provider_id,
                model: agent.model,
                effort: agent.effort,
                kickoff_prompt: agent.kickoff_prompt,
                sessions,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(state: Option<&str>, awaiting: Option<&str>) -> SessionActivityInput {
        SessionActivityInput {
            session_updated_at: String::new(),
            projection_state: "completed".into(),
            active_turn_id: None,
            projection_updated_at: None,
            turn_count: 1,
            last_turn_state: state.map(str::to_string),
            last_turn_started_at: None,
            last_turn_completed_at: None,
            last_turn_awaiting_user: awaiting.map(str::to_string),
            last_turn_awaiting_since: None,
            usage_limit: None,
        }
    }

    #[test]
    fn a_session_reads_like_the_desktop() {
        assert_eq!(session_state(None), AgentState::Idle);
        assert_eq!(
            session_state(Some(&input(Some("running"), None))),
            AgentState::Working
        );
        assert_eq!(
            session_state(Some(&input(Some("running"), Some("permission")))),
            AgentState::NeedsYou
        );
        assert_eq!(
            session_state(Some(&input(Some("completed"), None))),
            AgentState::Done
        );
        assert_eq!(
            session_state(Some(&input(Some("aborted"), None))),
            AgentState::Idle
        );
    }

    #[test]
    fn the_most_urgent_session_wins_and_read_results_rest() {
        use AgentState::*;
        assert_eq!(aggregate(&[(Done, true), (Working, false)]), Working);
        assert_eq!(aggregate(&[(Working, false), (NeedsYou, false)]), NeedsYou);
        assert_eq!(aggregate(&[(Done, false)]), Idle);
        assert_eq!(aggregate(&[(Done, true)]), Done);
        assert!(is_result_unread(Some("2026-10-23T10:00:00Z"), None));
        assert!(!is_result_unread(
            Some("2026-10-23T10:00:00Z"),
            Some("2026-10-23T10:00:00Z")
        ));
        assert!(!is_result_unread(None, None));
        let agents = [Idle, Working, Done];
        assert_eq!(lead(&agents, |state| *state), Some(&Done));
        assert_eq!(lead(&[Idle], |state| *state), None);
    }
}
