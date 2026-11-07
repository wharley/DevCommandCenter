//! A task's conversation: which session the desktop opens for it and that
//! session's history, projected into timeline rows. Blocking: call it off
//! the UI thread.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use dcc_core::domain::attention::SessionActivityInput;
use dcc_core::domain::session::{
    QueuedTurn, SessionEventRecord, SessionId, WorkspaceSessionSummary,
};
use dcc_core::domain::timeline::activity::{TurnTokenTotals, total_turn_tokens};
use dcc_core::domain::timeline::{TimelineMessage, project_session};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_core::ports::{DelegationRepo, RepositoryRepo, SessionRepo, UsageRepo, WorkspaceRepo};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use futures::executor::block_on;

pub use super::delegation_record::{ChildSession, DelegationRecord};

/// How often the checkout's branch is read from git.
const GIT_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

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
    /// The project's checkout and the task's worktree, apart (the terminal
    /// dock's two scopes).
    pub root_path: Option<String>,
    pub worktree_path: Option<String>,
    /// The branch the task was made from (`workspace.baseBranch`).
    pub base_branch: Option<String>,
    /// The task's name and branch (`source.headBranch ?? baseBranch`), for
    /// a provider handoff.
    pub workspace_name: Option<String>,
    pub branch: Option<String>,
    /// What the rail under the composer shows (`ExecutionContextRail`).
    pub context: Option<ExecutionContext>,
    /// The plan limit that stopped the latest turn, while it stands.
    pub usage_limit: Option<dcc_core::domain::usage_limit::SessionUsageLimit>,
    /// The provider's own goal (`/goal`), while the session has one.
    pub goal: Option<dcc_core::domain::goal::SessionGoal>,
    /// "Continuar sozinho se o limite acabar" for this conversation.
    pub auto_resume: bool,
    /// Follow-ups waiting for the running turn, in order.
    pub queue: Arc<Vec<QueuedTurn>>,
    /// No conversation yet: the first prompt starts one.
    pub start: Option<ThreadStart>,
    /// How another conversation of the task starts (a snippet sent to a
    /// "Nova sessão"), whether or not it has one.
    pub new_thread: Option<ThreadStart>,
    /// The task's project (`repository.projectId`).
    pub project_id: Option<String>,
    /// The DCC's own durable objective (the composer's "Objetivo").
    pub objective: Option<dcc_core::domain::objective::SessionObjective>,
    pub rows: Arc<Vec<TimelineMessage>>,
    /// The task's delegations by id, for the cards' live status and report.
    pub delegations: HashMap<String, DelegationRecord>,
    /// The task's conversations for the header's history.
    pub sessions: Arc<Vec<crate::data::session_list::SessionRow>>,
    /// Child conversations' state and title, by session id.
    pub children: HashMap<String, ChildSession>,
    /// Provider-reported tokens by turn, for the turn footer. Read while no
    /// turn runs, like the desktop.
    pub turn_tokens: Arc<HashMap<String, TurnTokenTotals>>,
    /// Highest event sequence read, so a refresh only reads what is new.
    pub last_sequence: u64,
    /// The task's repository (`currentRepository.id`), for the forge's
    /// binding after a sign-in.
    pub repository_id: Option<String>,
    /// What the Session tab's runtime shows (`sessionSnapshot`).
    pub facts: Option<SessionFacts>,
    /// The session's last 12 events since this run first read it (the
    /// desktop's live `activityEvents`), for the Session tab.
    pub activity: Arc<Vec<SessionEventRecord>>,
}

/// `SessionSnapshot`, what "Runtime da sessão" reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFacts {
    /// `idle` | `active` | `completed` | `aborted`
    pub state: String,
    pub turn_count: u32,
    pub checkpoint_count: u32,
    pub last_turn_prompt: Option<String>,
    pub provider_id: String,
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
    queue: Arc<Vec<QueuedTurn>>,
    /// The checkout's branch and when git was last asked.
    current_branch: Option<(std::time::Instant, Option<String>)>,
    /// Per session, the sequence read first in this run: what came after it
    /// is the live activity.
    live_from: HashMap<String, u64>,
}

/// A task with no conversation yet: what its first prompt starts one with
/// (`startThread`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadStart {
    pub workspace_id: String,
    pub project_id: String,
    /// The other projects of a multi-project task (its bundle's members).
    pub additional_workspace_ids: Vec<String>,
    /// How the sidebar names it: its own name, else its PR or branch title.
    pub name: Option<String>,
    /// Still a placeholder ("Nova tarefa"): the first prompt names it.
    pub auto_named: bool,
}

/// The task's place: its project, branch and checkout.
#[derive(Clone, Debug, Default)]
pub struct ExecutionContext {
    pub project_label: Option<String>,
    pub project_icon: Option<String>,
    pub project_color: Option<String>,
    pub project_seed: Option<String>,
    /// The branch the task was made on (`source.headBranch ?? baseBranch`).
    pub base_branch: Option<String>,
    /// What git says now; read every few seconds.
    pub current_branch: Option<String>,
    /// A protected worktree, not the project's own checkout.
    pub isolated: bool,
    pub setup_report: Option<dcc_core::domain::workspace::WorkspaceSetupReport>,
    /// The first of the task's ports (`DCC_PORT`); none once it is finished.
    pub port: Option<u16>,
}

impl PartialEq for ExecutionContext {
    fn eq(&self, other: &Self) -> bool {
        self.project_label == other.project_label
            && self.project_icon == other.project_icon
            && self.project_color == other.project_color
            && self.base_branch == other.base_branch
            && self.current_branch == other.current_branch
            && self.isolated == other.isolated
            && self.port == other.port
            && format!("{:?}", self.setup_report) == format!("{:?}", other.setup_report)
    }
}

impl Eq for ExecutionContext {}

/// `effectiveSelectedSessionId` with nothing picked yet: the first open
/// session that is not a delegated child, else the first open one.
/// The task's port block, taken now if it has none: what its terminals and
/// agents get anyway. A finished task takes none.
fn task_port(
    workspaces: &SqliteWorkspaceRepo,
    record: &dcc_core::domain::workspace::Workspace,
) -> Option<u16> {
    use dcc_core::domain::workspace::WorkspaceState;
    if matches!(
        record.state,
        WorkspaceState::Completed | WorkspaceState::Archived
    ) {
        return None;
    }
    workspaces.workspace_port_base(&record.id).ok()
}

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

/// A session's events from the database (the merge resolver follows its
/// agent's turn with them).
pub fn session_events(
    db_path: &std::path::Path,
    session_id: &str,
) -> anyhow::Result<Vec<SessionEventRecord>> {
    let sessions = SqliteSessionRepo::open_read_only(db_path)?;
    read_events(&sessions, &SessionId(session_id.to_string()), 0)
}

/// A task's `ThreadStart` read again (the quick composer's new task: its
/// bundle's other projects come along).
pub fn thread_start_for(db_path: &Path, workspace_id: &str) -> anyhow::Result<ThreadStart> {
    let workspaces = SqliteWorkspaceRepo::open(db_path)?;
    let id = dcc_core::domain::workspace::WorkspaceId(workspace_id.to_string());
    let record = block_on(workspaces.get_workspace(&id))?
        .ok_or_else(|| anyhow::anyhow!("the task {workspace_id} is gone"))?;
    thread_start(&workspaces, &record)
}

/// What the first prompt of a task with no conversation starts it with.
fn thread_start(
    workspaces: &SqliteWorkspaceRepo,
    record: &dcc_core::domain::workspace::Workspace,
) -> anyhow::Result<ThreadStart> {
    use dcc_core::domain::task_title::is_automatic_task_title;
    use dcc_core::ports::WorkspaceBundleRepo;
    let bundle = block_on(workspaces.get_workspace_bundle_for_workspace(&record.id))?
        .filter(|bundle| bundle.bundle.primary_workspace_id == record.id);
    let explicit = record
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let source = record.source.as_ref().and_then(|source| {
        source
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .or_else(|| Some(source.head_branch.trim()).filter(|branch| !branch.is_empty()))
    });
    // `workspaceToSummary().isAutoNamed`; a multi-project task goes by its bundle.
    let auto_named = match &bundle {
        Some(bundle) => is_automatic_task_title(Some(&bundle.bundle.name)),
        None => {
            (explicit.is_none() && source.is_none())
                || explicit.is_some_and(|name| is_automatic_task_title(Some(name)))
        }
    };
    Ok(ThreadStart {
        workspace_id: record.id.0.clone(),
        project_id: record.project_id.0.clone(),
        additional_workspace_ids: bundle
            .as_ref()
            .map(|bundle| {
                bundle
                    .members
                    .iter()
                    .map(|member| member.workspace_id.0.clone())
                    .filter(|id| *id != record.id.0)
                    .collect()
            })
            .unwrap_or_default(),
        name: bundle
            .map(|bundle| bundle.bundle.name)
            .or_else(|| explicit.or(source).map(str::to_string)),
        auto_named,
    })
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
    let lineage: HashMap<String, String> = delegations
        .iter()
        .filter_map(|delegation| {
            delegation
                .child_session_id
                .as_ref()
                .map(|child| (child.0.clone(), delegation.parent_session_id.0.clone()))
        })
        .collect();
    conversation.sessions = Arc::new(crate::data::session_list::rows(&summaries, &lineage));
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
    conversation
        .children
        .extend(super::task_proposal::own_task_children(
            &sessions,
            &delegations,
        ));
    for record in conversation.delegations.values_mut() {
        record.worktree = record.child_session_id.as_ref().and_then(|child| {
            summaries
                .iter()
                .find(|summary| &summary.session.id.0 == child)
                .and_then(|summary| summary.session.working_directory_override.clone())
                .filter(|path| !path.trim().is_empty())
        });
    }
    let wanted = wanted_session.and_then(|id| {
        summaries
            .iter()
            .find(|summary| summary.session.id.0 == id && summary.thread.archived_at.is_none())
    });
    let summary = wanted.or_else(|| choose_session(&summaries, &delegated));
    let workspaces = SqliteWorkspaceRepo::open(db_path)?;
    let record = block_on(workspaces.get_workspace(&workspace))?;
    if let Some(record) = &record {
        // `source.headBranch ?? baseBranch`
        let branch = record
            .source
            .as_ref()
            .map(|source| source.head_branch.clone())
            .unwrap_or_else(|| record.base_branch.clone());
        let path = record
            .worktree_path
            .clone()
            .unwrap_or_else(|| record.root_path.clone());
        let repositories = block_on(workspaces.list_repositories())?;
        let repository = repositories
            .iter()
            .find(|repository| repository.root_path == record.root_path);
        conversation.repository_id = repository.map(|repository| repository.id.0.clone());
        let project = repository.map(crate::data::tasks::Project::from_repository);
        conversation.context = Some(ExecutionContext {
            project_label: project.as_ref().map(|project| project.display_name.clone()),
            project_icon: project.as_ref().and_then(|project| project.icon.clone()),
            project_color: project.as_ref().and_then(|project| project.color.clone()),
            project_seed: Some(record.root_path.clone()),
            base_branch: Some(branch.clone()),
            current_branch: None,
            isolated: record.worktree_path.is_some(),
            setup_report: record.setup_report.clone(),
            port: task_port(&workspaces, record),
        });
        conversation.workspace_name = record.name.clone();
        conversation.project_id = Some(record.project_id.0.clone());
        conversation.branch = Some(branch);
        conversation.workspace_path = Some(path);
        conversation.root_path = Some(record.root_path.clone());
        conversation.worktree_path = record.worktree_path.clone();
        conversation.base_branch = Some(record.base_branch.clone());
    }
    conversation.new_thread = record
        .as_ref()
        .map(|record| thread_start(&workspaces, record))
        .transpose()?;
    let Some(summary) = summary else {
        conversation.start = conversation.new_thread.clone();
        return Ok(conversation);
    };
    let activity = SessionActivityInput::from_summary(summary);
    let session_id = summary.session.id.0.clone();
    conversation.session_id = Some(session_id.clone());
    conversation.provider_id = Some(summary.session.provider_id.clone());
    conversation.facts = Some(SessionFacts {
        state: match summary.projection.state {
            dcc_core::domain::session::SessionState::Draft => "idle",
            dcc_core::domain::session::SessionState::Active => "active",
            dcc_core::domain::session::SessionState::Completed => "completed",
            dcc_core::domain::session::SessionState::Aborted => "aborted",
        }
        .into(),
        turn_count: summary.projection.turn_count,
        checkpoint_count: summary.projection.checkpoint_count,
        last_turn_prompt: summary.last_turn_prompt.clone(),
        provider_id: summary.projection.provider_id.clone(),
    });
    conversation.model = summary.session.model.clone();
    conversation.running = activity.is_running();
    conversation.waiting = activity.waiting_reason().is_some();
    conversation.aborted =
        summary.session.state == dcc_core::domain::session::SessionState::Aborted;
    conversation.usage_limit = summary.usage_limit.clone();
    conversation.goal = summary.goal.clone();
    conversation.objective = sessions.load_session_objective(&summary.session.id)?;

    if cache.session_id.as_deref() != Some(session_id.as_str()) {
        *cache = EventCache {
            session_id: Some(session_id.clone()),
            live_from: std::mem::take(&mut cache.live_from),
            ..EventCache::default()
        };
    }
    let fresh = read_events(&sessions, &summary.session.id, cache.last_sequence)?;
    let first_read = !cache.live_from.contains_key(&session_id);
    if let Some(last) = fresh.last() {
        cache.last_sequence = last.sequence;
        cache.events.extend(fresh);
        cache.rows = Arc::new(project_session(&cache.events, &session_id));
        let queue = dcc_core::application::project_turn_queue(&cache.events);
        if *cache.queue != queue {
            cache.queue = Arc::new(queue);
        }
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
    // The session's own choice, else the global preference.
    conversation.auto_resume = dcc_core::domain::usage_limit::session_auto_resume_choice(
        &cache.events,
    )
    .unwrap_or_else(|| {
        db_path
            .parent()
            .is_some_and(|dir| dcc_infra::usage_limit_settings::load_settings(dir).auto_resume)
    });
    if let (Some(context), Some(root)) = (
        conversation.context.as_mut(),
        conversation.workspace_path.as_deref(),
    ) {
        let stale = cache
            .current_branch
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() >= GIT_EVERY);
        if stale {
            // One `rev-parse`, not a whole status: "HEAD" when detached.
            let branch = dcc_infra::workspace_git::resolve_current_branch_name(root).ok();
            cache.current_branch = Some((std::time::Instant::now(), branch));
        }
        context.current_branch = cache
            .current_branch
            .as_ref()
            .and_then(|(_, branch)| branch.clone());
    }
    conversation.rows = cache.rows.clone();
    conversation.queue = cache.queue.clone();
    conversation.turn_tokens = cache.turn_tokens.clone();
    conversation.last_sequence = cache.last_sequence;
    if first_read {
        cache
            .live_from
            .insert(session_id.clone(), cache.last_sequence);
    }
    let from = cache
        .live_from
        .get(&session_id)
        .copied()
        .unwrap_or(u64::MAX);
    let live: Vec<_> = cache
        .events
        .iter()
        .filter(|event| event.sequence > from)
        .collect();
    conversation.activity = Arc::new(
        live[live.len().saturating_sub(MAX_ACTIVITY)..]
            .iter()
            .map(|event| (*event).clone())
            .collect(),
    );
    Ok(conversation)
}

/// `MAX_ACTIVITY_EVENTS`
const MAX_ACTIVITY: usize = 12;

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
            && self.root_path == other.root_path
            && self.worktree_path == other.worktree_path
            && self.base_branch == other.base_branch
            && self.delegations == other.delegations
            && self.children == other.children
            && self.sessions == other.sessions
            && self.turn_tokens == other.turn_tokens
            && self.workspace_name == other.workspace_name
            && self.branch == other.branch
            && self.context == other.context
            && self.usage_limit == other.usage_limit
            && format!("{:?}", self.goal) == format!("{:?}", other.goal)
            && self.auto_resume == other.auto_resume
            && self.objective == other.objective
            && self.queue == other.queue
            && self.start == other.start
            && self.project_id == other.project_id
    }
}
