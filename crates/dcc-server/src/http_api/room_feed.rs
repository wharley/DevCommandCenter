//! The rooms as the phone reads them (`docs/PLANO_MOBILE_SALAS`): the
//! agents, each one's conversations with the room they are held in and
//! whether a turn runs there, read from DCC's database the way the desktop's
//! team column does. A conversation's lines are read whole once and kept
//! until its history moves, so a room read again costs only what changed.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dcc_core::domain::agent::{ResidentAgent, ReviewerPresetText};
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::attention::{AttentionReason, SessionActivityInput};
use dcc_core::domain::session::{SessionEventRecord, SessionId};
use dcc_core::domain::team_room_lines::{lines_of, RoomLine};
use dcc_core::domain::team_room_live::moves_only_the_turn;
use dcc_core::domain::timeline::activity::total_turn_tokens;
use dcc_core::domain::timeline::{project_session, TimelineMessage};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::UsageRepo;
use dcc_infra::db::{AgentSessionBinding, SqliteSessionRepo};

/// How many of a room's latest lines the phone gets.
pub(super) const ROOM_LINES: usize = 60;
const EVENT_PAGE: usize = 500;
/// Conversations whose lines are kept between reads.
const KEPT_CONVERSATIONS: usize = 64;

/// The agents, their conversations and how each one is.
pub(super) struct Team {
    pub agents: Vec<ResidentAgent>,
    /// Every conversation bound to a live agent (its own and its tasks').
    pub bindings: Vec<AgentSessionBinding>,
    pub activity: HashMap<String, SessionActivityInput>,
}

pub(super) fn read_team(repo: &SqliteSessionRepo) -> Result<Team, String> {
    let agents = repo
        .list_stored_resident_agents(&ReviewerPresetText::default())
        .map_err(|error| error.to_string())?;
    let bindings: Vec<AgentSessionBinding> = repo
        .list_agent_session_bindings()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|binding| agents.iter().any(|agent| agent.id == binding.agent_id))
        .collect();
    let mut workspaces: Vec<&str> = bindings
        .iter()
        .map(|binding| binding.workspace_id.as_str())
        .collect();
    workspaces.sort_unstable();
    workspaces.dedup();
    let mut activity = HashMap::new();
    for workspace in workspaces {
        for summary in repo
            .list_workspace_sessions(&WorkspaceId(workspace.to_string()))
            .map_err(|error| error.to_string())?
        {
            activity.insert(
                summary.session.id.0.clone(),
                SessionActivityInput::from_summary(&summary),
            );
        }
    }
    Ok(Team {
        agents,
        bindings,
        activity,
    })
}

impl Team {
    /// Whether `binding` is one of its agent's own conversations (not a task).
    pub fn is_home(binding: &AgentSessionBinding) -> bool {
        binding.project_id == home_project_id(&binding.agent_id)
    }

    /// When a conversation last moved, as the desktop's column reads it:
    /// its session's time or its last turn's start, end or question.
    pub fn changed_at(&self, binding: &AgentSessionBinding) -> String {
        let input = self.activity.get(&binding.session_id);
        [
            Some(binding.updated_at.as_str()),
            input.map(|input| input.session_updated_at.as_str()),
            input.and_then(|input| input.last_turn_started_at.as_deref()),
            input.and_then(|input| input.last_turn_completed_at.as_deref()),
            input.and_then(|input| input.last_turn_awaiting_since.as_deref()),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or_default()
        .to_string()
    }

    /// The conversations of `room` (none: Geral), least recently moved first.
    pub fn room_homes(&self, room: Option<&str>) -> Vec<&AgentSessionBinding> {
        let mut homes: Vec<(String, &AgentSessionBinding)> = self
            .bindings
            .iter()
            .filter(|binding| Self::is_home(binding) && binding.room.as_deref() == room)
            .map(|binding| (self.changed_at(binding), binding))
            .collect();
        homes.sort_by(|left, right| left.0.cmp(&right.0));
        homes.into_iter().map(|(_, binding)| binding).collect()
    }

    pub fn running(&self, binding: &AgentSessionBinding) -> bool {
        self.activity
            .get(&binding.session_id)
            .is_some_and(SessionActivityInput::is_running)
    }

    /// Mid-turn and waiting on the person (a permission or a question).
    pub fn waiting(&self, binding: &AgentSessionBinding) -> Option<AttentionReason> {
        self.activity
            .get(&binding.session_id)
            .filter(|input| input.is_running())
            .and_then(SessionActivityInput::waiting_reason)
    }

    /// The agents with one of `homes` mid-turn, each once, in the team's order.
    pub fn working(&self, homes: &[&AgentSessionBinding]) -> Vec<String> {
        let busy: HashSet<&str> = homes
            .iter()
            .filter(|binding| self.running(binding))
            .map(|binding| binding.agent_id.as_str())
            .collect();
        self.agents
            .iter()
            .filter(|agent| busy.contains(agent.id.as_str()))
            .map(|agent| agent.id.clone())
            .collect()
    }

    /// `agent_id`'s newest conversation in `room`, if it has one.
    pub fn conversation(&self, agent_id: &str, room: Option<&str>) -> Option<String> {
        self.room_homes(room)
            .into_iter()
            .rev()
            .find(|binding| binding.agent_id == agent_id)
            .map(|binding| binding.session_id.clone())
    }

    /// `agent_id`'s newest conversation in any room.
    pub fn latest_conversation(&self, agent_id: &str) -> Option<String> {
        self.bindings
            .iter()
            .filter(|binding| binding.agent_id == agent_id && Self::is_home(binding))
            .max_by_key(|binding| self.changed_at(binding))
            .map(|binding| binding.session_id.clone())
    }

    /// What changes when the team or one of `homes` moves: a turn starts or
    /// ends, a question is asked or answered. Nothing here reads a history.
    pub fn digest(&self, homes: &[&AgentSessionBinding], digest: &mut sha2::Sha256) {
        use sha2::Digest;
        for agent in &self.agents {
            digest.update(agent.id.as_bytes());
            digest.update(agent.name.as_bytes());
        }
        for home in homes {
            digest.update(home.session_id.as_bytes());
            digest.update(self.changed_at(home).as_bytes());
            if let Some(input) = self.activity.get(&home.session_id) {
                digest.update([u8::from(input.is_running())]);
                for part in [
                    input.active_turn_id.as_deref(),
                    input.last_turn_state.as_deref(),
                    input.last_turn_awaiting_user.as_deref(),
                ] {
                    digest.update(part.unwrap_or("-").as_bytes());
                }
                digest.update(input.turn_count.to_le_bytes());
            }
        }
    }
}

pub(super) type Stamp = (i64, i64, i64);

fn kept() -> &'static std::sync::Mutex<HashMap<String, (Stamp, Arc<Vec<TimelineMessage>>)>> {
    static KEPT: std::sync::OnceLock<
        std::sync::Mutex<HashMap<String, (Stamp, Arc<Vec<TimelineMessage>>)>>,
    > = std::sync::OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// A conversation's messages, read whole only when its history moved.
pub(super) fn messages(
    repo: &SqliteSessionRepo,
    session_id: &str,
) -> Result<Arc<Vec<TimelineMessage>>, String> {
    let id = SessionId(session_id.to_string());
    let stamp = repo
        .session_event_stamp(&id)
        .map_err(|error| error.to_string())?;
    if let Some((at, messages)) = kept()
        .lock()
        .ok()
        .and_then(|kept| kept.get(session_id).cloned())
    {
        if at == stamp {
            return Ok(messages);
        }
        // While a turn streams only its own work moves: the lines, read
        // from settled answers, stay as they were.
        let fresh = events_after(repo, &id, u64::try_from(at.1).unwrap_or(0))?;
        if only_the_turn_moved(at, stamp, &fresh) {
            if let Ok(mut kept) = kept().lock() {
                kept.insert(session_id.to_string(), (stamp, messages.clone()));
            }
            return Ok(messages);
        }
    }
    let events = events_after(repo, &id, 0)?;
    let messages = Arc::new(project_session(&events, session_id));
    if let Ok(mut kept) = kept().lock() {
        if kept.len() >= KEPT_CONVERSATIONS && !kept.contains_key(session_id) {
            kept.clear();
        }
        kept.insert(session_id.to_string(), (stamp, messages.clone()));
    }
    Ok(messages)
}

/// A conversation's events after `cursor`, in pages.
pub(super) fn events_after(
    repo: &SqliteSessionRepo,
    id: &SessionId,
    mut cursor: u64,
) -> Result<Vec<SessionEventRecord>, String> {
    let mut events = Vec::new();
    loop {
        let page = repo
            .list_events_by_session_after_sync(id, cursor, EVENT_PAGE)
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
    Ok(events)
}

/// Whether going from `before` to `after` only appended `fresh`, and each
/// of them only moves an open turn (`moves_only_the_turn`).
fn only_the_turn_moved(before: Stamp, after: Stamp, fresh: &[SessionEventRecord]) -> bool {
    !fresh.is_empty()
        && before.0 + fresh.len() as i64 == after.0
        && fresh.iter().all(|event| moves_only_the_turn(&event.kind))
}

/// One conversation's lines, with its turns' tokens read fresh.
pub(super) fn conversation_lines(
    repo: &SqliteSessionRepo,
    agent_id: &str,
    session_id: &str,
) -> Result<Vec<RoomLine>, String> {
    let messages = messages(repo, session_id)?;
    let id = SessionId(session_id.to_string());
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
    Ok(lines_of(agent_id, session_id, &messages, &tokens))
}

/// The conversations' lines in time order, the latest `ROOM_LINES`.
pub(super) fn room_lines(
    repo: &SqliteSessionRepo,
    homes: &[&AgentSessionBinding],
) -> Result<Vec<RoomLine>, String> {
    let mut lines = Vec::new();
    for home in homes {
        lines.extend(conversation_lines(repo, &home.agent_id, &home.session_id)?);
    }
    Ok(latest(lines, ROOM_LINES))
}

/// `lines` in time order, the last `count`.
pub(super) fn latest(mut lines: Vec<RoomLine>, count: usize) -> Vec<RoomLine> {
    lines.sort_by(|left, right| left.at.cmp(&right.at));
    let skip = lines.len().saturating_sub(count);
    lines.split_off(skip)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use dcc_core::domain::agent::AgentAvatar;
    use dcc_core::domain::team_room_lines::Speaker;

    pub(crate) fn agent(id: &str) -> ResidentAgent {
        ResidentAgent {
            id: id.into(),
            name: format!("Agente {id}"),
            role: String::new(),
            kickoff_prompt: String::new(),
            offer_prompt: String::new(),
            extra_instructions: String::new(),
            provider_id: None,
            model: None,
            effort: None,
            avatar: AgentAvatar {
                shape: "octopus".to_string(),
                accessory: "none".to_string(),
                color: "violet".into(),
                arms: 3,
                eyes: "round".into(),
            },
            projects: Vec::new(),
            monthly_token_limit: None,
            tool_access: Default::default(),
            auto_start_tasks: false,
            may_call: Vec::new(),
            account_id: None,
            limit_override_month: None,
            preset: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// A conversation of `agent`: its own in `room` (`Some`), else a task's
    /// in `project`.
    pub(crate) fn binding(
        session: &str,
        agent: &str,
        room: Option<Option<&str>>,
        at: &str,
    ) -> AgentSessionBinding {
        AgentSessionBinding {
            session_id: session.into(),
            agent_id: agent.into(),
            workspace_id: format!("w-{session}"),
            project_id: match room {
                Some(_) => home_project_id(agent),
                None => "checkout".into(),
            },
            title: None,
            updated_at: at.into(),
            room: room.flatten().map(str::to_string),
        }
    }

    pub(crate) fn input(running: bool, waiting: Option<&str>) -> SessionActivityInput {
        SessionActivityInput {
            session_updated_at: String::new(),
            projection_state: if running { "active" } else { "completed" }.into(),
            active_turn_id: running.then(|| "t".to_string()),
            projection_updated_at: None,
            turn_count: 1,
            last_turn_state: Some(if running { "running" } else { "completed" }.into()),
            last_turn_started_at: Some("2026-11-10T10:00:00Z".into()),
            last_turn_completed_at: None,
            last_turn_awaiting_user: waiting.map(str::to_string),
            last_turn_awaiting_since: None,
            usage_limit: None,
        }
    }

    pub(crate) fn team() -> Team {
        Team {
            agents: vec![agent("dev"), agent("rev")],
            bindings: vec![
                binding("s-geral", "dev", Some(None), "2026-11-10T09:00:00Z"),
                binding(
                    "s-old",
                    "rev",
                    Some(Some("checkout")),
                    "2026-11-09T09:00:00Z",
                ),
                binding(
                    "s-new",
                    "rev",
                    Some(Some("checkout")),
                    "2026-11-10T08:00:00Z",
                ),
                binding(
                    "s-dev-room",
                    "dev",
                    Some(Some("checkout")),
                    "2026-11-08T08:00:00Z",
                ),
                binding("s-task", "dev", None, "2026-11-10T11:00:00Z"),
            ],
            activity: HashMap::from([
                ("s-dev-room".to_string(), input(true, Some("permission"))),
                ("s-task".to_string(), input(true, None)),
            ]),
        }
    }

    #[test]
    fn a_rooms_conversations_and_who_works_there() {
        let team = team();
        let room: Vec<&str> = team
            .room_homes(Some("checkout"))
            .iter()
            .map(|binding| binding.session_id.as_str())
            .collect();
        // The running one moved last (its turn started at 10:00).
        assert_eq!(room, ["s-old", "s-new", "s-dev-room"]);
        let homes = team.room_homes(Some("checkout"));
        assert_eq!(team.working(&homes), ["dev"]);
        assert_eq!(team.waiting(homes[2]), Some(AttentionReason::Permission));
        assert!(team.working(&team.room_homes(None)).is_empty());
        assert_eq!(
            team.conversation("rev", Some("checkout")).as_deref(),
            Some("s-new")
        );
        assert_eq!(team.conversation("rev", None), None);
        assert_eq!(
            team.latest_conversation("dev").as_deref(),
            Some("s-dev-room")
        );
    }

    #[test]
    fn the_digest_moves_with_a_turn_not_with_its_steps() {
        let mut team = team();
        let hash = |team: &Team| {
            use sha2::Digest;
            let mut digest = sha2::Sha256::new();
            team.digest(&team.room_homes(Some("checkout")), &mut digest);
            digest.finalize().to_vec()
        };
        let before = hash(&team);
        // A step inside the turn: the projection moves, the digest not.
        team.activity
            .get_mut("s-dev-room")
            .unwrap()
            .projection_updated_at = Some("2026-11-10T10:00:05Z".into());
        assert_eq!(hash(&team), before);
        // The question answered: it moves.
        team.activity
            .get_mut("s-dev-room")
            .unwrap()
            .last_turn_awaiting_user = None;
        assert_ne!(hash(&team), before);
    }

    #[test]
    fn the_latest_lines_in_time_order() {
        let line = |text: &str, at: &str| RoomLine {
            key: text.into(),
            agent_id: "dev".into(),
            session_id: "s".into(),
            speaker: Speaker::Agent,
            text: text.into(),
            at: at.into(),
            receipt: None,
            tasks: Vec::new(),
            steps: Default::default(),
        };
        let lines = latest(
            vec![
                line("c", "2026-11-10T10:03:00Z"),
                line("a", "2026-11-10T10:01:00Z"),
                line("b", "2026-11-10T10:02:00Z"),
            ],
            2,
        );
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["b", "c"]);
    }

    #[test]
    fn a_streamed_step_keeps_the_lines_read() {
        let event = |sequence: u64, json: serde_json::Value| SessionEventRecord {
            event_id: format!("e-{sequence}"),
            session_id: SessionId("s".into()),
            sequence,
            occurred_at: "2026-11-10T10:00:00Z".into(),
            kind: serde_json::from_value(json).unwrap(),
        };
        let delta = event(
            4,
            serde_json::json!({ "type": "turn_delta", "turnId": "t", "content": "oi" }),
        );
        let ended = event(
            5,
            serde_json::json!({ "type": "turn_completed", "turnId": "t" }),
        );
        assert!(only_the_turn_moved((3, 3, 3), (4, 4, 9), &[delta.clone()]));
        assert!(!only_the_turn_moved(
            (3, 3, 3),
            (5, 5, 9),
            &[delta.clone(), ended]
        ));
        // Something went missing in between: read it whole.
        assert!(!only_the_turn_moved((3, 3, 3), (6, 6, 9), &[delta]));
        assert!(!only_the_turn_moved((3, 3, 3), (3, 3, 3), &[]));
    }
}
