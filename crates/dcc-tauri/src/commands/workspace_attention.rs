//! Blockers the sidebar's "Needs you" section shows for every active task.
//!
//! The rail rows poll git and the forge only for the rows on screen, so a
//! blocker in a collapsed project would never surface. This computes the few
//! states that truly block the person — local merge conflicts, a PR in
//! conflict with its base, failing checks, delegated edits awaiting review —
//! for all active tasks in one call, with a short PR-status cache so the
//! forge is not hit once per task per poll.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use specta::Type;
#[cfg(feature = "tauri")]
use tauri::State;

use dcc_core::{
    domain::{delegation::DelegationStatus, workspace::WorkspaceState},
    ports::{DelegationRepo, WorkspaceRepo},
};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};

use crate::{
    commands::{
        forge_commands::{workspace_pr_status_with_state, WorkspacePrStatusInput},
        workspace_commands::workspace_git_status_inner,
    },
    state::WorkspaceCommandState,
};

const PR_STATUS_TTL: Duration = Duration::from_secs(120);

pub use dcc_core::domain::attention::{
    attention_items, classify_git_blockers, derive_agent_activity, timestamp_ms, AgentActivity,
    BlockerFirstSeen,
    AgentActivityState, AttentionItem, AttentionReason, AttentionWorkspace, PrSnapshot,
    SessionActivityInput, WorkspaceAttentionBlockersOutput, WorkspaceBlocker, WorkspaceBlockerKind,
};





fn blocker_first_seen() -> &'static Mutex<BlockerFirstSeen> {
    static FIRST_SEEN: OnceLock<Mutex<BlockerFirstSeen>> = OnceLock::new();
    FIRST_SEEN.get_or_init(|| Mutex::new(BlockerFirstSeen::default()))
}

fn pr_cache() -> &'static Mutex<HashMap<String, (Instant, PrSnapshot)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, PrSnapshot)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn cached_pr_status(
    state: &WorkspaceCommandState,
    root: &str,
    branch: &str,
) -> Option<PrSnapshot> {
    let key = format!("{root}\u{1f}{branch}");
    if let Some((at, view)) = pr_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        if at.elapsed() < PR_STATUS_TTL {
            return Some(view);
        }
    }
    let output = workspace_pr_status_with_state(
        state,
        WorkspacePrStatusInput {
            workspace_root: root.to_string(),
            branch: Some(branch.to_string()),
            forge_login: None,
        },
    )
    .await
    .ok()?;
    let view = PrSnapshot {
        number: output.number.map(u64::from),
        head_branch: output.head_branch,
        state: output.state,
        mergeable: output.mergeable,
        merge_state_status: output.merge_state_status,
    };
    if let Ok(mut cache) = pr_cache().lock() {
        cache.insert(key, (Instant::now(), view.clone()));
    }
    Some(view)
}

pub async fn workspace_attention_blockers_with_state(
    state: &WorkspaceCommandState,
) -> Result<WorkspaceAttentionBlockersOutput, String> {
    let workspaces = SqliteWorkspaceRepo::open(&state.db_path)
        .map_err(|error| error.to_string())?
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    let mut blockers = Vec::new();

    let reviews = SqliteSessionRepo::open(&state.db_path)
        .map_err(|error| error.to_string())?
        .list_delegations_by_status(DelegationStatus::ReviewPending)
        .await
        .map_err(|error| error.to_string())?;
    let mut review_counts = HashMap::<String, u64>::new();
    for delegation in reviews {
        *review_counts.entry(delegation.workspace_id.0).or_default() += 1;
    }

    for workspace in workspaces {
        if matches!(
            workspace.state,
            WorkspaceState::Archived | WorkspaceState::Completed
        ) {
            continue;
        }
        if let Some(count) = review_counts.get(&workspace.id.0) {
            blockers.push(WorkspaceBlocker {
                workspace_id: workspace.id.0.clone(),
                kind: WorkspaceBlockerKind::DelegatedEditsReview,
                count: Some(*count),
                since: None,
            });
        }
        let root = workspace
            .worktree_path
            .clone()
            .unwrap_or_else(|| workspace.root_path.clone());
        if !Path::new(&root).is_dir() {
            continue;
        }
        let status_root = root.clone();
        let Ok(Ok(status)) =
            tokio::task::spawn_blocking(move || workspace_git_status_inner(&status_root)).await
        else {
            continue;
        };
        let pr = match status.current_branch.as_deref() {
            Some(branch) if status.conflict_count == 0 => {
                cached_pr_status(state, &root, branch).await
            }
            _ => None,
        };
        if let Some((kind, count)) = classify_git_blockers(
            status.conflict_count,
            status.merge_in_progress,
            status.current_branch.as_deref(),
            pr.as_ref(),
        ) {
            blockers.push(WorkspaceBlocker {
                workspace_id: workspace.id.0.clone(),
                kind,
                count,
                since: None,
            });
        }
    }
    if let Ok(mut first_seen) = blocker_first_seen().lock() {
        first_seen.stamp(&mut blockers, &chrono::Utc::now().to_rfc3339());
    }
    Ok(WorkspaceAttentionBlockersOutput { blockers })
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAttentionEntry {
    #[serde(flatten)]
    pub item: AttentionItem,
    /// The session to open: the one waiting or running, else the newest.
    pub session_id: Option<String>,
}

/// A task with its agent at work: the desktop sidebar's "N rodando" counts
/// these, one per task however many of its sessions run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunningTask {
    pub workspace_id: String,
    /// The session to open: the running one, newest first.
    pub session_id: String,
    /// Every running session of the task.
    pub session_ids: Vec<String>,
    pub started_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceAttentionOutput {
    pub items: Vec<WorkspaceAttentionEntry>,
    /// Tasks whose agent is working (not waiting on the person), newest
    /// start first.
    #[serde(default)]
    pub running: Vec<RunningTask>,
    /// The open permission and question events of the tasks waiting on the
    /// person, so a client answers them without reading whole histories.
    #[serde(default)]
    pub requests: Vec<dcc_core::domain::session::SessionEventRecord>,
}

fn blockers_cache() -> &'static Mutex<Option<(Instant, Vec<WorkspaceBlocker>)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Vec<WorkspaceBlocker>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The blockers, reused for `max_age` (they run git for every task).
pub async fn recent_attention_blockers(
    state: &WorkspaceCommandState,
    max_age: Duration,
) -> Result<Vec<WorkspaceBlocker>, String> {
    if let Some((at, blockers)) = blockers_cache().lock().ok().and_then(|cache| cache.clone()) {
        if at.elapsed() < max_age {
            return Ok(blockers);
        }
    }
    let blockers = workspace_attention_blockers_with_state(state)
        .await?
        .blockers;
    if let Ok(mut cache) = blockers_cache().lock() {
        *cache = Some((Instant::now(), blockers.clone()));
    }
    Ok(blockers)
}

/// The phone's "Precisa de você": every active task's reason, newest return
/// first. Blockers no older than `blockers_max_age`.
pub async fn workspace_attention_with_state(
    state: &WorkspaceCommandState,
    blockers_max_age: Duration,
) -> Result<WorkspaceAttentionOutput, String> {
    let workspaces = SqliteWorkspaceRepo::open(&state.db_path)
        .map_err(|error| error.to_string())?
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?;
    let repo = SqliteSessionRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let mut tasks = Vec::new();
    let mut activities = HashMap::new();
    let mut sessions_to_open = HashMap::new();
    let mut running = Vec::new();
    let mut requests = Vec::new();
    for workspace in &workspaces {
        if matches!(
            workspace.state,
            WorkspaceState::Archived | WorkspaceState::Completed
        ) {
            continue;
        }
        let summaries = repo
            .list_workspace_sessions(&workspace.id)
            .map_err(|error| error.to_string())?;
        let inputs: Vec<SessionActivityInput> = summaries
            .iter()
            .map(SessionActivityInput::from_summary)
            .collect();
        let open = inputs
            .iter()
            .position(|input| input.is_running() && input.waiting_reason().is_some())
            .or_else(|| inputs.iter().position(SessionActivityInput::is_running))
            .unwrap_or(0);
        if let Some(summary) = summaries.get(open) {
            sessions_to_open.insert(workspace.id.0.clone(), summary.session.id.0.clone());
        }
        for (summary, input) in summaries.iter().zip(&inputs) {
            if input.is_running() && input.waiting_reason().is_some() {
                let events = repo
                    .list_events_by_session_sync(&summary.session.id)
                    .map_err(|error| error.to_string())?;
                requests.extend(
                    dcc_core::domain::session::open_turn_request_events(&events)
                        .into_iter()
                        .cloned(),
                );
            }
        }
        if let Some(activity) = derive_agent_activity(&inputs) {
            if activity.state == AgentActivityState::Active {
                let session_ids: Vec<String> = summaries
                    .iter()
                    .zip(&inputs)
                    .filter(|(_, input)| input.is_running())
                    .map(|(summary, _)| summary.session.id.0.clone())
                    .collect();
                if let Some(session_id) = session_ids.first().cloned() {
                    running.push(RunningTask {
                        workspace_id: workspace.id.0.clone(),
                        session_id,
                        session_ids,
                        started_at: activity.started_at.clone(),
                    });
                }
            }
            activities.insert(workspace.id.0.clone(), activity);
        }
        tasks.push(AttentionWorkspace::from_workspace(workspace));
    }
    running.sort_by(|left, right| {
        let left_ms = timestamp_ms(left.started_at.as_deref()).unwrap_or(0);
        let right_ms = timestamp_ms(right.started_at.as_deref()).unwrap_or(0);
        right_ms
            .cmp(&left_ms)
            .then_with(|| left.workspace_id.cmp(&right.workspace_id))
    });
    let blockers = recent_attention_blockers(state, blockers_max_age).await?;
    let items = attention_items(
        &tasks,
        &activities,
        &blockers,
        chrono::Utc::now().timestamp_millis(),
    )
    .into_iter()
    .map(|item| WorkspaceAttentionEntry {
        session_id: sessions_to_open.get(&item.workspace_id).cloned(),
        item,
    })
    .collect();
    Ok(WorkspaceAttentionOutput {
        items,
        running,
        requests,
    })
}

#[cfg(feature = "tauri")]
#[tauri::command]
pub async fn workspace_attention_blockers(
    state: State<'_, WorkspaceCommandState>,
) -> Result<WorkspaceAttentionBlockersOutput, String> {
    workspace_attention_blockers_with_state(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocker(workspace: &str, kind: WorkspaceBlockerKind) -> WorkspaceBlocker {
        WorkspaceBlocker {
            workspace_id: workspace.to_string(),
            kind,
            count: None,
            since: None,
        }
    }

    #[test]
    fn first_seen_leaves_startup_blockers_unstamped_and_stamps_new_ones() {
        let mut first_seen = BlockerFirstSeen::default();
        let mut startup = vec![blocker("a", WorkspaceBlockerKind::Conflicts)];
        first_seen.stamp(&mut startup, "2026-10-06T09:00:00Z");
        assert_eq!(startup[0].since, None);

        let mut later = vec![
            blocker("a", WorkspaceBlockerKind::Conflicts),
            blocker("b", WorkspaceBlockerKind::ChecksFailing),
        ];
        first_seen.stamp(&mut later, "2026-10-06T09:01:00Z");
        assert_eq!(later[0].since, None);
        assert_eq!(later[1].since.as_deref(), Some("2026-10-06T09:01:00Z"));

        let mut still = vec![blocker("b", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut still, "2026-10-06T09:02:00Z");
        assert_eq!(still[0].since.as_deref(), Some("2026-10-06T09:01:00Z"));
    }

    #[test]
    fn first_seen_stamps_a_blocker_again_when_it_comes_back() {
        let mut first_seen = BlockerFirstSeen::default();
        first_seen.stamp(&mut [], "2026-10-06T09:00:00Z");
        let mut checks = vec![blocker("a", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut checks, "2026-10-06T09:01:00Z");
        first_seen.stamp(&mut [], "2026-10-06T09:02:00Z");
        let mut again = vec![blocker("a", WorkspaceBlockerKind::ChecksFailing)];
        first_seen.stamp(&mut again, "2026-10-06T09:03:00Z");
        assert_eq!(again[0].since.as_deref(), Some("2026-10-06T09:03:00Z"));
    }
}
