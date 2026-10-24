//! When the person moves a conversation to another provider, the first turn
//! there carries a bounded re-anchor (`buildProviderHandoffContext`): the
//! task, its git state, the first mission spec, the plan still open and the
//! recent durable messages. Read off the UI thread at send time, like the
//! desktop does.

use std::path::PathBuf;
use std::sync::Arc;

use dcc_core::domain::timeline::{Role, TimelineMessage};
use dcc_core::domain::turn_instructions::{
    HandoffChange, HandoffGit, HandoffMessage, HandoffRole, ProviderHandoffInput,
    build_provider_handoff_context, should_create_provider_handoff,
};
use dcc_core::domain::workspace_git::WorkspaceGitChangeEntry;

/// What the send knows when it starts.
pub(crate) struct HandoffRequest {
    pub source_provider: Option<String>,
    pub destination_provider: String,
    pub workspace_name: Option<String>,
    pub workspace_path: Option<String>,
    pub branch: Option<String>,
    pub history: Arc<Vec<TimelineMessage>>,
    pub prompt: String,
    pub db_path: PathBuf,
}

fn message(row: &TimelineMessage) -> HandoffMessage {
    HandoffMessage {
        role: match row.role {
            Role::User => HandoffRole::User,
            Role::Assistant => HandoffRole::Assistant,
            Role::System => HandoffRole::System,
        },
        content: row.content.clone(),
        label: (!row.label.is_empty()).then(|| row.label.clone()),
        streaming: row.streaming,
        incomplete: row.status.is_some(),
    }
}

/// `derivePlanFollowUpState().activePlanMessage`: the last plan answer,
/// while nothing came after it.
fn active_plan(rows: &[TimelineMessage]) -> Option<String> {
    let index = rows
        .iter()
        .rposition(|row| row.answers_plan_mode && row.role == Role::Assistant)?;
    let consumed = rows[index + 1..]
        .iter()
        .any(|row| matches!(row.role, Role::User | Role::Assistant) || row.delegation.is_some());
    (!consumed).then(|| rows[index].content.clone())
}

fn changes(entries: &[WorkspaceGitChangeEntry]) -> Vec<HandoffChange> {
    entries
        .iter()
        .map(|entry| HandoffChange {
            path: entry.path.clone(),
            status: entry.status.clone(),
            insertions: entry.insertions,
            deletions: entry.deletions,
        })
        .collect()
}

/// The re-anchor for the destination provider, or `None` when the switch
/// carries nothing (same provider, no turns, nothing durable yet).
pub(crate) fn build(request: &HandoffRequest) -> Option<String> {
    let messages: Vec<HandoffMessage> = request.history.iter().map(message).collect();
    let turns = request
        .history
        .iter()
        .filter(|row| row.role == Role::User && row.turn_id.is_some())
        .count();
    if !should_create_provider_handoff(
        request.source_provider.as_deref(),
        turns,
        Some(&request.destination_provider),
        &messages,
    ) {
        return None;
    }
    let root = request.workspace_path.as_deref();
    // Git and the spec are extras: their failure never blocks the turn.
    let git = root.and_then(|root| {
        let status = dcc_infra::workspace_git::read_workspace_git_status(root).ok()?;
        let diff = futures::executor::block_on(
            dcc_infra::workspace_git::workspace_git_branch_diff(&request.db_path, root),
        )
        .ok()?;
        Some(HandoffGit {
            current_branch: status.current_branch,
            base_branch: diff.base_branch,
            staged: changes(&status.staged),
            unstaged: changes(&status.unstaged),
            branch_diff: changes(&diff.changes),
        })
    });
    let mission = root.and_then(dcc_infra::mission_specs::first_spec_content);
    let plan = active_plan(&request.history);
    Some(build_provider_handoff_context(&ProviderHandoffInput {
        source_provider_id: request.source_provider.as_deref().unwrap_or("unknown"),
        destination_provider_id: &request.destination_provider,
        workspace_name: request.workspace_name.as_deref(),
        workspace_path: root,
        branch: request.branch.as_deref(),
        git: git.as_ref(),
        mission_spec: mission.as_deref(),
        active_plan: plan.as_deref(),
        recent_messages: &messages,
        current_prompt: Some(&request.prompt),
    }))
}
