//! "Precisa de você": which tasks need the person and why, plus the agent
//! activity each task's sessions add up to. Pure rules shared by every
//! client (desktop rail, phone, native app); the desktop's TypeScript copy
//! runs the same `attention-vectors.json` cases.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::{
    usage_limit::{usage_limit_phase, SessionUsageLimit, UsageLimitPauseReason, UsageLimitPhase},
    workspace::WorkspaceState,
};

/// The fields of a task's PR status the blocker rules read.
#[derive(Clone, Debug, Default)]
pub struct PrSnapshot {
    pub number: Option<u64>,
    pub head_branch: Option<String>,
    pub state: Option<String>,
    pub mergeable: Option<String>,
    pub merge_state_status: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceBlockerKind {
    /// Local merge conflicts in the task's checkout.
    Conflicts,
    /// The task's PR conflicts with its base branch.
    PrConflicts,
    /// The task's PR has failing checks.
    ChecksFailing,
    /// Edits from a delegation wait for the person to apply or discard.
    DelegatedEditsReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBlocker {
    pub workspace_id: String,
    pub kind: WorkspaceBlockerKind,
    /// PR number for PR blockers; count for delegated reviews.
    pub count: Option<u64>,
    /// When this app first saw the blocker; null when it was already there at
    /// startup, so its real start is unknown.
    pub since: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAttentionBlockersOutput {
    pub blockers: Vec<WorkspaceBlocker>,
}

/// When each blocker first showed up, so "Needs you" can order tasks by when
/// they came back to the person.
#[derive(Clone, Debug, Default)]
pub struct BlockerFirstSeen {
    primed: bool,
    seen: HashMap<(String, WorkspaceBlockerKind), Option<String>>,
}

impl BlockerFirstSeen {
    /// Stamps the blockers found in this pass and forgets the ones that went
    /// away, so a blocker that comes back is stamped again. Blockers present
    /// on the first pass get no time: stamping them "now" would put blockers
    /// older than the app above fresh results.
    pub fn stamp(&mut self, blockers: &mut [WorkspaceBlocker], now: &str) {
        let mut next = HashMap::new();
        for blocker in blockers.iter_mut() {
            let key = (blocker.workspace_id.clone(), blocker.kind);
            let since = match self.seen.get(&key) {
                Some(since) => since.clone(),
                None if self.primed => Some(now.to_string()),
                None => None,
            };
            blocker.since = since.clone();
            next.insert(key, since);
        }
        self.seen = next;
        self.primed = true;
    }
}

/// Classifies one task's git and PR state; pure so the rules are testable.
pub fn classify_git_blockers(
    conflict_count: u32,
    merge_in_progress: bool,
    current_branch: Option<&str>,
    pr: Option<&PrSnapshot>,
) -> Option<(WorkspaceBlockerKind, Option<u64>)> {
    // A merge with every conflict resolved only needs to be completed; the
    // recap already offers that, so only open conflicts block.
    let _ = merge_in_progress;
    if conflict_count > 0 {
        return Some((WorkspaceBlockerKind::Conflicts, None));
    }
    let pr = pr?;
    let belongs = match (pr.head_branch.as_deref(), current_branch) {
        (Some(head), Some(current)) => head.trim() == current.trim(),
        (None, _) => true,
        _ => false,
    };
    if !belongs || pr.state.as_deref().map(str::to_lowercase).as_deref() != Some("open") {
        return None;
    }
    let number = pr.number;
    if pr.mergeable.as_deref() == Some("CONFLICTING")
        || pr
            .merge_state_status
            .as_deref()
            .map(str::to_uppercase)
            .as_deref()
            == Some("DIRTY")
    {
        return Some((WorkspaceBlockerKind::PrConflicts, number));
    }
    // UNSTABLE is GitHub's "failing commit status". BLOCKED also covers
    // "review required", which is not a failure, so it is left out.
    if pr
        .merge_state_status
        .as_deref()
        .map(str::to_uppercase)
        .as_deref()
        == Some("UNSTABLE")
    {
        return Some((WorkspaceBlockerKind::ChecksFailing, number));
    }
    None
}

// ---------- "Precisa de você", for the phone ----------
//
// The desktop computes this list in its sidebar
// (`apps/desktop/src/features/workspaces/use-workspace-agent-states.ts`,
// `deriveAgentActivityFromSessions` + `attentionWorkspaceItems`). The phone
// reads it from here (`GET /api/v1/mobile/attention`). Both run the cases in
// `crates/dcc-core/fixtures/attention-vectors.json`, so a change to one
// rule without the other fails a test. Whether a finished result was seen is
// each client's own store, so `completed`/`aborted` come back always and the
// client drops the ones it has opened.

/// Why a task needs the person, best first (the desktop's `ATTENTION_ORDER`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum AttentionReason {
    Permission,
    Input,
    /// Stopped by the plan's usage limit with nothing that resumes it by
    /// itself ("Pausado por limite"). Waiting for a reset is not here.
    UsageLimit,
    Conflicts,
    PrConflicts,
    ChecksFailing,
    DelegatedReview,
    Woke,
    Setup,
    Aborted,
    Completed,
}

impl AttentionReason {
    fn from_blocker(kind: WorkspaceBlockerKind) -> Self {
        match kind {
            WorkspaceBlockerKind::Conflicts => Self::Conflicts,
            WorkspaceBlockerKind::PrConflicts => Self::PrConflicts,
            WorkspaceBlockerKind::ChecksFailing => Self::ChecksFailing,
            WorkspaceBlockerKind::DelegatedEditsReview => Self::DelegatedReview,
        }
    }
}

/// What the rule reads from one of the task's sessions
/// (`WorkspaceSessionSummary`), newest session first.
#[derive(Clone, Debug, Default)]
pub struct SessionActivityInput {
    pub session_updated_at: String,
    /// `SessionState` in snake_case: `active`, `completed`, `aborted`, `draft`.
    pub projection_state: String,
    pub active_turn_id: Option<String>,
    pub projection_updated_at: Option<String>,
    pub turn_count: u32,
    pub last_turn_state: Option<String>,
    pub last_turn_started_at: Option<String>,
    pub last_turn_completed_at: Option<String>,
    pub last_turn_awaiting_user: Option<String>,
    pub last_turn_awaiting_since: Option<String>,
    pub usage_limit: Option<SessionUsageLimit>,
}

impl SessionActivityInput {
    pub fn from_summary(summary: &crate::domain::session::WorkspaceSessionSummary) -> Self {
        use crate::domain::session::SessionState;
        let projection_state = match summary.projection.state {
            SessionState::Draft => "draft",
            SessionState::Active => "active",
            SessionState::Completed => "completed",
            SessionState::Aborted => "aborted",
        }
        .to_string();
        Self {
            session_updated_at: summary.session.updated_at.clone(),
            projection_state,
            active_turn_id: summary
                .projection
                .active_turn_id
                .as_ref()
                .map(|id| id.0.clone()),
            projection_updated_at: Some(summary.projection.updated_at.clone()),
            turn_count: summary.projection.turn_count,
            last_turn_state: summary.last_turn_state.clone(),
            last_turn_started_at: summary.last_turn_started_at.clone(),
            last_turn_completed_at: summary.last_turn_completed_at.clone(),
            last_turn_awaiting_user: summary.last_turn_awaiting_user.clone(),
            last_turn_awaiting_since: summary.last_turn_awaiting_since.clone(),
            usage_limit: summary.usage_limit.clone(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.last_turn_state.as_deref() == Some("running")
            || (self.projection_state == "active"
                && self
                    .active_turn_id
                    .as_deref()
                    .is_some_and(|id| !id.is_empty()))
    }

    pub fn waiting_reason(&self) -> Option<AttentionReason> {
        match self.last_turn_awaiting_user.as_deref() {
            Some("permission") => Some(AttentionReason::Permission),
            Some("input") => Some(AttentionReason::Input),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentActivityState {
    Active,
    Waiting(AttentionReason),
    /// The latest turn stopped on a usage limit; whether that waits for the
    /// reset or for the person depends on the clock (`usage_limit_phase`).
    UsageLimited,
    Completed,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentActivity {
    pub state: AgentActivityState,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub waiting_since: Option<String>,
    pub usage_limit: Option<SessionUsageLimit>,
}

impl AgentActivity {
    /// The agent is mid-task: running, waiting on the person, or stopped by
    /// a usage limit it continues after.
    pub fn turn_open(&self) -> bool {
        matches!(
            self.state,
            AgentActivityState::Active
                | AgentActivityState::Waiting(_)
                | AgentActivityState::UsageLimited
        )
    }
}

/// `deriveAgentActivityFromSessions`: the running (or waiting) session wins,
/// else the newest session's last outcome.
pub fn derive_agent_activity(sessions: &[SessionActivityInput]) -> Option<AgentActivity> {
    let running: Vec<&SessionActivityInput> = sessions.iter().filter(|s| s.is_running()).collect();
    let picked = running
        .iter()
        .find(|s| s.waiting_reason().is_some())
        .or_else(|| running.first())
        .copied();
    if let Some(session) = picked {
        let started_at = session
            .last_turn_started_at
            .clone()
            .or_else(|| session.projection_updated_at.clone())
            .or_else(|| Some(session.session_updated_at.clone()));
        return Some(match session.waiting_reason() {
            Some(reason) => AgentActivity {
                state: AgentActivityState::Waiting(reason),
                started_at,
                completed_at: None,
                waiting_since: session.last_turn_awaiting_since.clone(),
                usage_limit: None,
            },
            None => AgentActivity {
                state: AgentActivityState::Active,
                started_at,
                completed_at: None,
                waiting_since: None,
                usage_limit: None,
            },
        });
    }
    let latest = sessions.first()?;
    let completed_at = latest
        .last_turn_completed_at
        .clone()
        .or_else(|| latest.projection_updated_at.clone())
        .or_else(|| Some(latest.session_updated_at.clone()));
    if let Some(limit) = &latest.usage_limit {
        return Some(AgentActivity {
            state: AgentActivityState::UsageLimited,
            started_at: latest.last_turn_started_at.clone(),
            completed_at,
            waiting_since: Some(limit.hit_at.clone()),
            usage_limit: Some(limit.clone()),
        });
    }
    if latest.last_turn_state.as_deref() == Some("aborted") || latest.projection_state == "aborted"
    {
        return Some(AgentActivity {
            state: AgentActivityState::Aborted,
            started_at: latest.last_turn_started_at.clone(),
            completed_at,
            waiting_since: None,
            usage_limit: None,
        });
    }
    if latest.last_turn_state.as_deref() == Some("completed")
        || latest.projection_state == "completed"
        || latest.turn_count > 0
    {
        return Some(AgentActivity {
            state: AgentActivityState::Completed,
            started_at: latest.last_turn_started_at.clone(),
            completed_at,
            waiting_since: None,
            usage_limit: None,
        });
    }
    None
}

/// The task fields the rule reads (`WorkspaceSummary` on the desktop).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionWorkspace {
    pub id: String,
    /// `ready`, `setup_pending`, `initializing`, `archived`, `completed`.
    pub status: String,
    #[serde(default)]
    pub snoozed_until: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl AttentionWorkspace {
    /// `workspaceToSummary`'s status: a `setup_pending` task only stays so
    /// with a setup step that warned or failed (or a failed report with no
    /// steps); older tasks persisted it for recommendations never required.
    pub fn from_workspace(workspace: &crate::domain::workspace::Workspace) -> Self {
        use crate::domain::workspace::WorkspaceSetupStatus;
        let has_setup_problem = workspace.worktree_path.is_some()
            && workspace.setup_report.as_ref().is_some_and(|report| {
                report.steps.iter().any(|step| {
                    step.command != "compile_mission_spec_context"
                        && step.command != "refresh_repository_forge_metadata"
                        && matches!(
                            step.status,
                            WorkspaceSetupStatus::Warning | WorkspaceSetupStatus::Failed
                        )
                }) || (matches!(report.status, WorkspaceSetupStatus::Failed)
                    && report.steps.is_empty())
            });
        let status = match workspace.state {
            WorkspaceState::Ready => "ready",
            WorkspaceState::Archived => "archived",
            WorkspaceState::Completed => "completed",
            WorkspaceState::Initializing => "initializing",
            WorkspaceState::SetupPending if has_setup_problem => "setup_pending",
            WorkspaceState::SetupPending => "ready",
        };
        Self {
            id: workspace.id.0.clone(),
            status: status.to_string(),
            snoozed_until: workspace.snoozed_until.clone(),
            created_at: Some(workspace.created_at.clone()),
            updated_at: Some(workspace.updated_at.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub workspace_id: String,
    pub reason: AttentionReason,
    /// PR number for PR blockers; count for delegated reviews.
    pub count: Option<u64>,
    /// When the task came back to the person for this reason; orders the list.
    pub returned_at: Option<String>,
    /// `completed`/`aborted`: when the result finished, for the client's seen store.
    pub completed_at: Option<String>,
}

pub fn timestamp_ms(value: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value?)
        .ok()
        .map(|time| time.timestamp_millis())
}

/// `attentionWorkspaceItems`, without the unread filter (see above). Newest
/// return first, then workspace id.
pub fn attention_items(
    workspaces: &[AttentionWorkspace],
    activities: &HashMap<String, AgentActivity>,
    blockers: &[WorkspaceBlocker],
    now_ms: i64,
) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = workspaces
        .iter()
        .filter_map(|workspace| {
            if workspace.status == "archived" || workspace.status == "completed" {
                return None;
            }
            let snoozed = timestamp_ms(workspace.snoozed_until.as_deref());
            if snoozed.is_some_and(|at| at > now_ms) {
                return None;
            }
            let activity = activities.get(&workspace.id);
            let item = |reason, count, returned_at: Option<String>, completed_at| AttentionItem {
                workspace_id: workspace.id.clone(),
                reason,
                count,
                returned_at,
                completed_at,
            };
            if let Some(AgentActivity {
                state: AgentActivityState::Waiting(reason),
                started_at,
                waiting_since,
                ..
            }) = activity
            {
                return Some(item(
                    *reason,
                    None,
                    waiting_since.clone().or_else(|| started_at.clone()),
                    None,
                ));
            }
            if let Some(AgentActivity {
                state: AgentActivityState::UsageLimited,
                usage_limit: Some(limit),
                ..
            }) = activity
            {
                let now = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms)
                    .unwrap_or_else(chrono::Utc::now);
                let status = usage_limit_phase(limit, now);
                if status.phase == UsageLimitPhase::Waiting {
                    return None;
                }
                // A reopened window came back to the person at the reset.
                let returned_at = match status.pause_reason {
                    Some(UsageLimitPauseReason::ResetPassed) => limit.resets_at.clone(),
                    _ => Some(limit.hit_at.clone()),
                };
                return Some(item(AttentionReason::UsageLimit, None, returned_at, None));
            }
            let turn_open = activity.is_some_and(AgentActivity::turn_open);
            if !turn_open {
                let blocker = blockers
                    .iter()
                    .filter(|blocker| blocker.workspace_id == workspace.id)
                    .min_by_key(|blocker| AttentionReason::from_blocker(blocker.kind));
                if let Some(blocker) = blocker {
                    return Some(item(
                        AttentionReason::from_blocker(blocker.kind),
                        blocker.count,
                        blocker
                            .since
                            .clone()
                            .or_else(|| workspace.updated_at.clone()),
                        None,
                    ));
                }
                if snoozed.is_some_and(|at| at <= now_ms) {
                    return Some(item(
                        AttentionReason::Woke,
                        None,
                        workspace.snoozed_until.clone(),
                        None,
                    ));
                }
                if workspace.status == "setup_pending" {
                    return Some(item(
                        AttentionReason::Setup,
                        None,
                        workspace
                            .updated_at
                            .clone()
                            .or_else(|| workspace.created_at.clone()),
                        None,
                    ));
                }
            }
            match activity {
                Some(AgentActivity {
                    state: AgentActivityState::Aborted,
                    completed_at,
                    ..
                }) => Some(item(
                    AttentionReason::Aborted,
                    None,
                    completed_at.clone(),
                    completed_at.clone(),
                )),
                Some(AgentActivity {
                    state: AgentActivityState::Completed,
                    completed_at,
                    ..
                }) => Some(item(
                    AttentionReason::Completed,
                    None,
                    completed_at.clone(),
                    completed_at.clone(),
                )),
                _ => None,
            }
        })
        .collect();
    items.sort_by(|left, right| {
        let left_ms = timestamp_ms(left.returned_at.as_deref()).unwrap_or(0);
        let right_ms = timestamp_ms(right.returned_at.as_deref()).unwrap_or(0);
        right_ms
            .cmp(&left_ms)
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn pr(state: &str, mergeable: Option<&str>, merge_state: Option<&str>) -> PrSnapshot {
        PrSnapshot {
            number: Some(12),
            head_branch: Some("feature".to_string()),
            state: Some(state.to_string()),
            mergeable: mergeable.map(str::to_string),
            merge_state_status: merge_state.map(str::to_string),
        }
    }

    #[test]
    fn local_conflicts_block_before_anything_else() {
        assert_eq!(
            classify_git_blockers(
                2,
                true,
                Some("feature"),
                Some(&pr("OPEN", None, Some("UNSTABLE")))
            ),
            Some((WorkspaceBlockerKind::Conflicts, None))
        );
    }

    #[test]
    fn open_prs_block_on_conflicts_or_failing_checks_only() {
        let classify = |pr: PrSnapshot| classify_git_blockers(0, false, Some("feature"), Some(&pr));
        assert_eq!(
            classify(pr("OPEN", Some("CONFLICTING"), Some("DIRTY"))),
            Some((WorkspaceBlockerKind::PrConflicts, Some(12)))
        );
        assert_eq!(
            classify(pr("OPEN", Some("MERGEABLE"), Some("UNSTABLE"))),
            Some((WorkspaceBlockerKind::ChecksFailing, Some(12)))
        );
        // Waiting for a required review is not a failure.
        assert_eq!(
            classify(pr("OPEN", Some("MERGEABLE"), Some("BLOCKED"))),
            None
        );
        assert_eq!(classify(pr("OPEN", Some("MERGEABLE"), Some("CLEAN"))), None);
        assert_eq!(
            classify(pr("MERGED", Some("CONFLICTING"), Some("DIRTY"))),
            None
        );
    }

    #[test]
    fn a_pr_from_another_branch_is_ignored() {
        let mut other = pr("OPEN", Some("CONFLICTING"), Some("DIRTY"));
        other.head_branch = Some("older/merged".to_string());
        assert_eq!(
            classify_git_blockers(0, false, Some("feature"), Some(&other)),
            None
        );
        assert_eq!(classify_git_blockers(0, false, Some("feature"), None), None);
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VectorSession {
        session: VectorSessionRow,
        projection: VectorProjection,
        last_turn_state: Option<String>,
        last_turn_started_at: Option<String>,
        last_turn_completed_at: Option<String>,
        last_turn_awaiting_user: Option<String>,
        last_turn_awaiting_since: Option<String>,
        #[serde(default)]
        usage_limit: Option<SessionUsageLimit>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VectorSessionRow {
        updated_at: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VectorProjection {
        state: String,
        active_turn_id: Option<String>,
        updated_at: Option<String>,
        turn_count: u32,
    }
    #[derive(Deserialize)]
    struct Vectors {
        now: String,
        unread: Vec<String>,
        workspaces: Vec<AttentionWorkspace>,
        sessions: HashMap<String, Vec<VectorSession>>,
        blockers: Vec<WorkspaceBlocker>,
        expected: Vec<serde_json::Value>,
    }

    #[test]
    fn attention_matches_the_desktop_on_the_shared_vectors() {
        let vectors: Vectors =
            serde_json::from_str(include_str!("../../fixtures/attention-vectors.json"))
                .expect("vectors");
        let activities: HashMap<String, AgentActivity> = vectors
            .sessions
            .iter()
            .filter_map(|(workspace_id, sessions)| {
                let inputs: Vec<SessionActivityInput> = sessions
                    .iter()
                    .map(|s| SessionActivityInput {
                        session_updated_at: s.session.updated_at.clone(),
                        projection_state: s.projection.state.clone(),
                        active_turn_id: s.projection.active_turn_id.clone(),
                        projection_updated_at: s.projection.updated_at.clone(),
                        turn_count: s.projection.turn_count,
                        last_turn_state: s.last_turn_state.clone(),
                        last_turn_started_at: s.last_turn_started_at.clone(),
                        last_turn_completed_at: s.last_turn_completed_at.clone(),
                        last_turn_awaiting_user: s.last_turn_awaiting_user.clone(),
                        last_turn_awaiting_since: s.last_turn_awaiting_since.clone(),
                        usage_limit: s.usage_limit.clone(),
                    })
                    .collect();
                derive_agent_activity(&inputs).map(|activity| (workspace_id.clone(), activity))
            })
            .collect();
        let now = chrono::DateTime::parse_from_rfc3339(&vectors.now)
            .unwrap()
            .timestamp_millis();
        let items: Vec<serde_json::Value> =
            attention_items(&vectors.workspaces, &activities, &vectors.blockers, now)
                .into_iter()
                // The client's seen store, as on the desktop.
                .filter(|item| {
                    !matches!(
                        item.reason,
                        AttentionReason::Completed | AttentionReason::Aborted
                    ) || vectors.unread.contains(&item.workspace_id)
                })
                .map(|item| {
                    serde_json::json!({
                        "workspaceId": item.workspace_id,
                        "reason": item.reason,
                        "count": item.count,
                        "returnedAt": item.returned_at,
                    })
                })
                .collect();
        assert_eq!(items, vectors.expected);
    }
}
