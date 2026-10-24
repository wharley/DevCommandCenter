//! A task's conversation: which session the desktop opens for it and that
//! session's history, projected into timeline rows. Blocking: call it off
//! the UI thread.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use dcc_core::domain::attention::SessionActivityInput;
use dcc_core::domain::delegation::{Delegation, DelegationKind, DelegationStatus};
use dcc_core::domain::session::{SessionEventRecord, SessionId, WorkspaceSessionSummary};
use dcc_core::domain::timeline::activity::{TurnTokenTotals, total_turn_tokens};
use dcc_core::domain::timeline::{TimelineMessage, project_session};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::{DelegationRepo, SessionRepo, UsageRepo, WorkspaceRepo};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use futures::executor::block_on;

/// Events read per query when catching up.
const EVENT_PAGE: usize = 5_000;

#[derive(Clone, Debug, Default)]
pub struct Conversation {
    pub task_id: String,
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    /// The latest turn is still running.
    pub running: bool,
    /// The latest turn waits on the person (a permission or a question).
    pub waiting: bool,
    /// The session was stopped; retry and continue reopen it first.
    pub aborted: bool,
    /// The task's checkout (`worktreePath ?? rootPath`): mentions list its
    /// files and attachments inside it go relative.
    pub workspace_path: Option<String>,
    /// The task's name and branch (`source.headBranch ?? baseBranch`), for
    /// a provider handoff.
    pub workspace_name: Option<String>,
    pub branch: Option<String>,
    pub rows: Arc<Vec<TimelineMessage>>,
    /// The task's delegations by id, for the cards' live status and report.
    pub delegations: HashMap<String, DelegationRecord>,
    /// Child conversations' state and title, by session id.
    pub children: HashMap<String, ChildSession>,
    /// Provider-reported tokens by turn, for the turn footer. Read while no
    /// turn runs, like the desktop.
    pub turn_tokens: Arc<HashMap<String, TurnTokenTotals>>,
    /// Highest event sequence read, so a refresh only reads what is new.
    pub last_sequence: u64,
}

/// The session history read so far, kept by whoever refreshes the
/// conversation so each refresh only reads the new events.
#[derive(Debug, Default)]
pub struct EventCache {
    session_id: Option<String>,
    events: Vec<SessionEventRecord>,
    last_sequence: u64,
    rows: Arc<Vec<TimelineMessage>>,
    turn_tokens: Arc<HashMap<String, TurnTokenTotals>>,
}

/// What a delegation card reads from the stored record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRecord {
    pub status: DelegationStatus,
    pub is_task: bool,
    pub title: Option<String>,
    pub instruction: String,
    pub result_summary: Option<String>,
    pub prompt: String,
    pub touched_files: Vec<String>,
    pub provider_id: String,
    pub model_id: Option<String>,
    pub child_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChildSession {
    pub title: String,
    pub busy: bool,
    pub waiting: bool,
}

/// `delegationInstruction`: the task as written, without DCC's wrapping.
fn delegation_instruction(delegation: &Delegation) -> String {
    if let Some(stored) = delegation
        .instruction
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return stored.to_string();
    }
    const MARKER: &str = "\nInstruction:\n";
    let Some(start) = delegation.prompt.find(MARKER) else {
        return delegation.prompt.trim().to_string();
    };
    let rest = &delegation.prompt[start + MARKER.len()..];
    let end = [
        "Git context",
        "Mission spec",
        "Recent parent session context",
    ]
    .iter()
    .filter_map(|name| rest.find(&format!("\n\n{name}:\n")))
    .min();
    end.map_or(rest, |end| &rest[..end]).trim().to_string()
}

impl DelegationRecord {
    fn from_delegation(delegation: &Delegation) -> Self {
        Self {
            status: delegation.status.clone(),
            is_task: delegation.kind == DelegationKind::Task,
            title: delegation.title.clone(),
            instruction: delegation_instruction(delegation),
            result_summary: delegation.result_summary.clone(),
            prompt: delegation.prompt.clone(),
            touched_files: delegation.touched_files.clone(),
            provider_id: delegation.target_provider_id.0.clone(),
            model_id: delegation.target_model_id.clone(),
            child_session_id: delegation.child_session_id.as_ref().map(|id| id.0.clone()),
        }
    }
}

/// `effectiveSelectedSessionId` with nothing picked yet: the first open
/// session that is not a delegated child, else the first open one.
pub fn choose_session<'a>(
    summaries: &'a [WorkspaceSessionSummary],
    delegated: &HashSet<String>,
) -> Option<&'a WorkspaceSessionSummary> {
    let mut visible = summaries
        .iter()
        .filter(|summary| summary.thread.archived_at.is_none());
    let first = visible.clone().next();
    visible
        .find(|summary| !delegated.contains(&summary.session.id.0))
        .or(first)
}

fn read_events(
    sessions: &SqliteSessionRepo,
    session_id: &SessionId,
    after: u64,
) -> anyhow::Result<Vec<SessionEventRecord>> {
    let mut events = Vec::new();
    let mut cursor = after;
    loop {
        let page = sessions.list_events_by_session_after_sync(session_id, cursor, EVENT_PAGE)?;
        let full = page.len() == EVENT_PAGE;
        if let Some(last) = page.last() {
            cursor = last.sequence;
        }
        events.extend(page);
        if !full {
            return Ok(events);
        }
    }
}

/// Reads the task's conversation. Only the events after those in `cache`
/// are read; the rows are projected again from all of them when any arrive.
pub fn load(
    db_path: &Path,
    task_id: &str,
    wanted_session: Option<&str>,
    cache: &mut EventCache,
) -> anyhow::Result<Conversation> {
    let mut conversation = Conversation {
        task_id: task_id.to_string(),
        ..Conversation::default()
    };
    if !db_path.exists() {
        return Ok(conversation);
    }
    let sessions = SqliteSessionRepo::open_read_only(db_path)?;
    let mut workspace = WorkspaceId(task_id.to_string());
    let mut summaries = sessions.list_workspace_sessions(&workspace)?;
    // A child's conversation can live in its own worktree's workspace.
    if let Some(wanted) = wanted_session
        && !summaries
            .iter()
            .any(|summary| summary.session.id.0 == wanted)
        && let Some(session) = block_on(sessions.get_session(&SessionId(wanted.to_string())))?
    {
        workspace = session.workspace_id;
        summaries = sessions.list_workspace_sessions(&workspace)?;
    }
    let delegations = block_on(sessions.list_delegations(Some(&workspace), None))?;
    let delegated: HashSet<String> = delegations
        .iter()
        .filter_map(|delegation| delegation.child_session_id.as_ref().map(|id| id.0.clone()))
        .collect();
    conversation.delegations = delegations
        .iter()
        .map(|delegation| {
            (
                delegation.id.0.clone(),
                DelegationRecord::from_delegation(delegation),
            )
        })
        .collect();
    conversation.children = summaries
        .iter()
        .filter(|summary| delegated.contains(&summary.session.id.0))
        .map(|summary| {
            let activity = SessionActivityInput::from_summary(summary);
            (
                summary.session.id.0.clone(),
                ChildSession {
                    title: summary.thread.title.clone(),
                    busy: activity.is_running(),
                    waiting: activity.waiting_reason().is_some(),
                },
            )
        })
        .collect();
    let wanted = wanted_session.and_then(|id| {
        summaries
            .iter()
            .find(|summary| summary.session.id.0 == id && summary.thread.archived_at.is_none())
    });
    let Some(summary) = wanted.or_else(|| choose_session(&summaries, &delegated)) else {
        return Ok(conversation);
    };
    let activity = SessionActivityInput::from_summary(summary);
    let session_id = summary.session.id.0.clone();
    conversation.workspace_path =
        block_on(SqliteWorkspaceRepo::open(db_path)?.get_workspace(&workspace))?
            .map(|workspace| workspace.worktree_path.unwrap_or(workspace.root_path));
    conversation.session_id = Some(session_id.clone());
    conversation.provider_id = Some(summary.session.provider_id.clone());
    conversation.model = summary.session.model.clone();
    conversation.running = activity.is_running();
    conversation.waiting = activity.waiting_reason().is_some();
    conversation.aborted =
        summary.session.state == dcc_core::domain::session::SessionState::Aborted;

    if cache.session_id.as_deref() != Some(session_id.as_str()) {
        *cache = EventCache {
            session_id: Some(session_id.clone()),
            ..EventCache::default()
        };
    }
    let fresh = read_events(&sessions, &summary.session.id, cache.last_sequence)?;
    if let Some(last) = fresh.last() {
        cache.last_sequence = last.sequence;
        cache.events.extend(fresh);
        cache.rows = Arc::new(project_session(&cache.events, &session_id));
    }
    if !conversation.running {
        let usage = block_on(sessions.list_session_turn_usage(&summary.session.id))?;
        let tokens: HashMap<String, TurnTokenTotals> = usage
            .iter()
            .filter_map(|turn| {
                total_turn_tokens(&turn.models).map(|totals| (turn.turn_id.clone(), totals))
            })
            .collect();
        if *cache.turn_tokens != tokens {
            cache.turn_tokens = Arc::new(tokens);
        }
    }
    conversation.rows = cache.rows.clone();
    conversation.turn_tokens = cache.turn_tokens.clone();
    conversation.last_sequence = cache.last_sequence;
    Ok(conversation)
}

impl Conversation {
    /// Same history and same state: nothing to redraw.
    pub fn same_as(&self, other: &Conversation) -> bool {
        self.task_id == other.task_id
            && self.session_id == other.session_id
            && self.last_sequence == other.last_sequence
            && self.running == other.running
            && self.waiting == other.waiting
            && self.aborted == other.aborted
            && self.workspace_path == other.workspace_path
            && self.delegations == other.delegations
            && self.children == other.children
            && self.turn_tokens == other.turn_tokens
    }
}
