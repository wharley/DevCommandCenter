use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use uuid::Uuid;

use dcc_core::{
    domain::{
        delegation::{
            Delegation, DelegationBudget, DelegationContextPolicy, DelegationId, DelegationMode,
            DelegationStatus,
        },
        delegation_worktree::{DelegationWorktreeOperationId, DelegationWorktreeOperationState},
        provider::ProviderId,
        session::{SessionEventKind, SessionEventRecord, SessionId, TurnId},
        workspace::WorkspaceId,
    },
    ports::{
        AppendEventOutcome, CoreEvent, DelegationRepo, DelegationWorktreeOperationRepo, EventBus,
        SessionEventRepo, SessionRepo,
    },
};

use crate::state::SessionCommandState;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateDelegationInput {
    pub parent_session_id: SessionId,
    pub parent_turn_id: Option<TurnId>,
    pub child_session_id: Option<SessionId>,
    #[serde(default)]
    pub delegation_worktree_operation_id: Option<String>,
    pub workspace_id: WorkspaceId,
    pub target_provider_id: ProviderId,
    #[serde(default)]
    pub target_model_id: Option<String>,
    pub mode: DelegationMode,
    pub prompt: String,
    #[serde(default)]
    pub context_policy: DelegationContextPolicy,
    #[serde(default)]
    pub budget: DelegationBudget,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateDelegationOutput {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListDelegationsInput {
    pub workspace_id: Option<WorkspaceId>,
    pub parent_session_id: Option<SessionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListDelegationsOutput {
    pub delegations: Vec<Delegation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GetDelegationInput {
    pub delegation_id: DelegationId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GetDelegationOutput {
    pub delegation: Option<Delegation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelDelegationInput {
    pub delegation_id: DelegationId,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelDelegationOutput {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartDelegationInput {
    pub delegation_id: DelegationId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StartDelegationOutput {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompleteDelegationInput {
    pub delegation_id: DelegationId,
    pub summary: Option<String>,
    #[serde(default)]
    pub touched_files: Vec<String>,
    pub diff_summary: Option<String>,
    pub validation_summary: Option<String>,
    #[serde(default)]
    pub review_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CompleteDelegationOutput {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApproveDelegationInput {
    pub delegation_id: DelegationId,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApproveDelegationOutput {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FailDelegationInput {
    pub delegation_id: DelegationId,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FailDelegationOutput {
    pub delegation: Delegation,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

async fn append_and_publish_parent_event(
    state: &SessionCommandState,
    session_id: &SessionId,
    kind: SessionEventKind,
    core_event: CoreEvent,
    occurred_at: String,
) -> Result<(), String> {
    let events = SessionEventRepo::list_events_by_session(state, session_id)
        .await
        .map_err(|error| error.to_string())?;
    let sequence = events.last().map(|event| event.sequence + 1).unwrap_or(1);
    let event = SessionEventRecord {
        event_id: Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        sequence,
        occurred_at,
        kind,
    };
    let outcome = SessionEventRepo::append_event(state, &event)
        .await
        .map_err(|error| error.to_string())?;
    if let AppendEventOutcome::Inserted(record) = outcome {
        EventBus::publish_durable_session(state, &record, core_event)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn is_terminal_status(status: &DelegationStatus) -> bool {
    matches!(
        status,
        DelegationStatus::Completed | DelegationStatus::Failed | DelegationStatus::Cancelled
    )
}

fn status_label(status: DelegationStatus) -> &'static str {
    match status {
        DelegationStatus::Draft => "draft",
        DelegationStatus::Queued => "queued",
        DelegationStatus::Running => "running",
        DelegationStatus::ReviewPending => "review_pending",
        DelegationStatus::Completed => "completed",
        DelegationStatus::Failed => "failed",
        DelegationStatus::Cancelled => "cancelled",
    }
}

fn validate_child_provider_target(
    child_session: &dcc_core::domain::session::Session,
    target: &ProviderId,
) -> Result<(), String> {
    if child_session.provider_id != target.0 {
        return Err("child session provider must match target_provider_id".to_string());
    }
    Ok(())
}

/// Rechecks the current provider authority before a legacy delegation is
/// allowed to transition. Older durable rows may have predated capability
/// validation, so this must run before status persistence or event fanout.
async fn validate_stored_delegation_target(
    state: &SessionCommandState,
    delegation: &Delegation,
) -> Result<(), String> {
    let parent_session = SessionRepo::get_session(state, &delegation.parent_session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "parent session not found: {}",
                delegation.parent_session_id.0
            )
        })?;
    if parent_session.workspace_id != delegation.workspace_id {
        return Err("delegation workspace_id must match the parent session workspace".to_string());
    }
    state
        .validate_delegation_target(
            &delegation.target_provider_id,
            &delegation.mode,
            &delegation.budget,
        )
        .map_err(|error| error.to_string())?;
    if let Some(child_session_id) = delegation.child_session_id.as_ref() {
        let child_session = SessionRepo::get_session(state, child_session_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("child session not found: {}", child_session_id.0))?;
        if child_session.workspace_id != delegation.workspace_id {
            return Err("child session must belong to delegation workspace_id".to_string());
        }
        validate_child_provider_target(&child_session, &delegation.target_provider_id)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn create_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: CreateDelegationInput,
) -> Result<CreateDelegationOutput, String> {
    if input.prompt.trim().is_empty() {
        return Err("prompt cannot be empty".to_string());
    }
    if input.target_provider_id.0.trim().is_empty() {
        return Err("target_provider_id cannot be empty".to_string());
    }
    state
        .validate_delegation_target(&input.target_provider_id, &input.mode, &input.budget)
        .map_err(|error| error.to_string())?;

    let parent_session = SessionRepo::get_session(&*state, &input.parent_session_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("parent session not found: {}", input.parent_session_id.0))?;
    if parent_session.workspace_id != input.workspace_id {
        return Err("workspace_id must match the parent session workspace".to_string());
    }
    let child_session = if let Some(child_session_id) = input.child_session_id.as_ref() {
        let child_session = SessionRepo::get_session(&*state, child_session_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("child session not found: {}", child_session_id.0))?;
        if child_session.workspace_id != input.workspace_id {
            return Err("child session must belong to workspace_id".to_string());
        }
        validate_child_provider_target(&child_session, &input.target_provider_id)?;
        // The child works toward the parent's objective with its own budget.
        // Best-effort: a failure here must not block creating the delegation.
        if let Err(error) =
            state.inherit_session_objective(&input.parent_session_id, child_session_id)
        {
            eprintln!("[DCC] delegation objective inheritance failed: {error}");
        }
        Some(child_session)
    } else {
        None
    };

    let now = now_iso();
    let delegation = Delegation {
        id: DelegationId(Uuid::new_v4().to_string()),
        parent_session_id: input.parent_session_id.clone(),
        parent_turn_id: input.parent_turn_id,
        child_session_id: input.child_session_id,
        workspace_id: input.workspace_id,
        target_provider_id: input.target_provider_id,
        target_model_id: input.target_model_id,
        mode: input.mode,
        status: DelegationStatus::Draft,
        prompt: input.prompt,
        context_policy: input.context_policy,
        budget: input.budget,
        result_summary: None,
        touched_files: Vec::new(),
        diff_summary: None,
        validation_summary: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    let edit_capable =
        matches!(delegation.mode, DelegationMode::Implement) || delegation.budget.allow_file_edits;
    let mut bound_operation = if let Some(operation_id) = input
        .delegation_worktree_operation_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let mut operation = DelegationWorktreeOperationRepo::get_delegation_worktree_operation(
            &*state,
            &DelegationWorktreeOperationId(operation_id.to_string()),
        )
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "delegation worktree journal entry was not found".to_string())?;
        if operation.state != DelegationWorktreeOperationState::Prepared {
            return Err(format!(
                "delegation worktree is {:?}, not prepared",
                operation.state
            ));
        }
        if operation.workspace_id != delegation.workspace_id
            || operation.parent_session_id.as_ref() != Some(&delegation.parent_session_id)
        {
            return Err("delegation worktree scope does not match the delegation".to_string());
        }
        let child_session_id = delegation
            .child_session_id
            .as_ref()
            .ok_or_else(|| "journaled implementation requires a child session".to_string())?;
        let child_session = child_session
            .as_ref()
            .ok_or_else(|| "journaled implementation requires a child session".to_string())?;
        if child_session.working_directory_override.as_deref()
            != Some(operation.worktree_path.as_str())
        {
            return Err(
                "child session working directory does not match the journaled worktree".to_string(),
            );
        }
        operation.delegation_id = Some(delegation.id.clone());
        operation.child_session_id = Some(child_session_id.clone());
        operation.state = DelegationWorktreeOperationState::Bound;
        operation.updated_at = now.clone();
        if !DelegationWorktreeOperationRepo::compare_and_swap_delegation_worktree_operation(
            &*state,
            DelegationWorktreeOperationState::Prepared,
            &operation,
        )
        .await
        .map_err(|error| error.to_string())?
        {
            return Err("delegation worktree journal changed while binding".to_string());
        }
        Some(operation)
    } else {
        if edit_capable {
            return Err(
                "edit-capable delegation requires a prepared worktree operation".to_string(),
            );
        }
        None
    };

    if let Err(error) = DelegationRepo::save_delegation(&*state, &delegation).await {
        if let Some(operation) = bound_operation.as_mut() {
            operation.state = DelegationWorktreeOperationState::CleanupRequired;
            operation.last_error = Some("failed to persist the bound delegation".to_string());
            operation.updated_at = now_iso();
            let _ =
                DelegationWorktreeOperationRepo::compare_and_swap_delegation_worktree_operation(
                    &*state,
                    DelegationWorktreeOperationState::Bound,
                    operation,
                )
                .await;
        }
        return Err(error.to_string());
    }
    if let Err(error) = append_and_publish_parent_event(
        &state,
        &delegation.parent_session_id,
        SessionEventKind::DelegationRequested {
            delegation_id: delegation.id.clone(),
        },
        CoreEvent::SessionDelegationRequested {
            session_id: delegation.parent_session_id.0.clone(),
            delegation_id: delegation.id.0.clone(),
        },
        now,
    )
    .await
    {
        // The delegation and its Bound journal entry are already durable.
        // Returning an error here would make the caller discard a valid
        // worktree, so event delivery is best-effort after persistence wins.
        eprintln!(
            "[DCC] delegation {} was saved but its requested event was not delivered: {}",
            delegation.id.0, error
        );
    }

    Ok(CreateDelegationOutput { delegation })
}

#[tauri::command]
pub async fn list_delegations(
    state: State<'_, SessionCommandState>,
    input: ListDelegationsInput,
) -> Result<ListDelegationsOutput, String> {
    let delegations = DelegationRepo::list_delegations(
        &*state,
        input.workspace_id.as_ref(),
        input.parent_session_id.as_ref(),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(ListDelegationsOutput { delegations })
}

#[tauri::command]
pub async fn get_delegation(
    state: State<'_, SessionCommandState>,
    input: GetDelegationInput,
) -> Result<GetDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(GetDelegationOutput { delegation })
}

#[tauri::command]
pub async fn cancel_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: CancelDelegationInput,
) -> Result<CancelDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    if is_terminal_status(&delegation.status) {
        return Err(format!(
            "delegation {} is already {}",
            delegation.id.0,
            status_label(delegation.status)
        ));
    }
    if matches!(delegation.status, DelegationStatus::ReviewPending)
        && (delegation.budget.allow_file_edits
            || matches!(delegation.mode, DelegationMode::Implement))
    {
        let operation =
            DelegationWorktreeOperationRepo::get_delegation_worktree_operation_by_delegation_id(
                &*state,
                &delegation.id,
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "edit-capable delegation has no worktree journal entry".to_string())?;
        if operation.state != DelegationWorktreeOperationState::Removed {
            return Err(
                "discard the isolated delegation worktree before cancelling review".to_string(),
            );
        }
    }
    let now = now_iso();
    let updated = DelegationRepo::update_delegation_status(
        &*state,
        &input.delegation_id,
        DelegationStatus::Cancelled,
        now.clone(),
    )
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    let reason = input.reason;
    append_and_publish_parent_event(
        &state,
        &updated.parent_session_id,
        SessionEventKind::DelegationCancelled {
            delegation_id: updated.id.clone(),
            reason: reason.clone(),
        },
        CoreEvent::SessionDelegationCancelled {
            session_id: updated.parent_session_id.0.clone(),
            delegation_id: updated.id.0.clone(),
            reason,
        },
        now,
    )
    .await?;

    Ok(CancelDelegationOutput {
        delegation: updated,
    })
}

#[tauri::command]
pub async fn start_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: StartDelegationInput,
) -> Result<StartDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    if is_terminal_status(&delegation.status) {
        return Err(format!(
            "delegation {} is already {}",
            delegation.id.0,
            status_label(delegation.status)
        ));
    }
    validate_stored_delegation_target(&state, &delegation).await?;
    if let Some(child_session_id) = delegation.child_session_id.as_ref() {
        // Covers children bound after creation; idempotent for the rest.
        if let Err(error) =
            state.inherit_session_objective(&delegation.parent_session_id, child_session_id)
        {
            eprintln!("[DCC] delegation objective inheritance failed: {error}");
        }
    }

    let now = now_iso();
    let updated = DelegationRepo::update_delegation_status(
        &*state,
        &input.delegation_id,
        DelegationStatus::Running,
        now.clone(),
    )
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    append_and_publish_parent_event(
        &state,
        &updated.parent_session_id,
        SessionEventKind::DelegationStarted {
            delegation_id: updated.id.clone(),
            child_session_id: updated.child_session_id.clone(),
        },
        CoreEvent::SessionDelegationStarted {
            session_id: updated.parent_session_id.0.clone(),
            delegation_id: updated.id.0.clone(),
            child_session_id: updated
                .child_session_id
                .as_ref()
                .map(|session_id| session_id.0.clone()),
        },
        now,
    )
    .await?;

    Ok(StartDelegationOutput {
        delegation: updated,
    })
}

#[tauri::command]
pub async fn complete_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: CompleteDelegationInput,
) -> Result<CompleteDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    if is_terminal_status(&delegation.status) {
        return Ok(CompleteDelegationOutput { delegation });
    }

    let now = now_iso();
    let mut updated = delegation;
    let review_required = input.review_required || updated.budget.allow_file_edits;
    updated.status = if review_required {
        DelegationStatus::ReviewPending
    } else {
        DelegationStatus::Completed
    };
    updated.updated_at = now.clone();
    updated.result_summary = input.summary.clone();
    updated.touched_files = input.touched_files;
    updated.diff_summary = input.diff_summary;
    updated.validation_summary = input.validation_summary;
    DelegationRepo::save_delegation(&*state, &updated)
        .await
        .map_err(|error| error.to_string())?;
    if review_required
        && (updated.budget.allow_file_edits || matches!(updated.mode, DelegationMode::Implement))
    {
        let mut operation =
            DelegationWorktreeOperationRepo::get_delegation_worktree_operation_by_delegation_id(
                &*state,
                &updated.id,
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "edit-capable delegation has no worktree journal entry".to_string())?;
        match operation.state {
            DelegationWorktreeOperationState::Bound => {
                operation.state = DelegationWorktreeOperationState::ReviewPending;
                operation.updated_at = now.clone();
                if !DelegationWorktreeOperationRepo::compare_and_swap_delegation_worktree_operation(
                    &*state,
                    DelegationWorktreeOperationState::Bound,
                    &operation,
                )
                .await
                .map_err(|error| error.to_string())?
                {
                    return Err(
                        "delegation worktree journal changed while entering review".to_string()
                    );
                }
            }
            DelegationWorktreeOperationState::ReviewPending => {}
            ref state => {
                return Err(format!(
                    "delegation worktree is {state:?}, not ready for review"
                ))
            }
        }
    }
    let summary = input.summary;
    if review_required {
        let content = "Delegated implementation finished and is awaiting human review.".to_string();
        append_and_publish_parent_event(
            &state,
            &updated.parent_session_id,
            SessionEventKind::DelegationDelta {
                delegation_id: updated.id.clone(),
                content: content.clone(),
            },
            CoreEvent::SessionDelegationDelta {
                session_id: updated.parent_session_id.0.clone(),
                delegation_id: updated.id.0.clone(),
                content,
            },
            now,
        )
        .await?;
    } else {
        append_and_publish_parent_event(
            &state,
            &updated.parent_session_id,
            SessionEventKind::DelegationCompleted {
                delegation_id: updated.id.clone(),
                summary: summary.clone(),
            },
            CoreEvent::SessionDelegationCompleted {
                session_id: updated.parent_session_id.0.clone(),
                delegation_id: updated.id.0.clone(),
                summary,
            },
            now,
        )
        .await?;
    }

    Ok(CompleteDelegationOutput {
        delegation: updated,
    })
}

#[tauri::command]
pub async fn approve_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: ApproveDelegationInput,
) -> Result<ApproveDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    if matches!(delegation.status, DelegationStatus::Completed) {
        return Ok(ApproveDelegationOutput { delegation });
    }
    if !matches!(delegation.status, DelegationStatus::ReviewPending) {
        return Err(format!(
            "delegation {} is {}, not review_pending",
            delegation.id.0,
            status_label(delegation.status)
        ));
    }

    if delegation.budget.allow_file_edits || matches!(delegation.mode, DelegationMode::Implement) {
        let operation =
            DelegationWorktreeOperationRepo::get_delegation_worktree_operation_by_delegation_id(
                &*state,
                &delegation.id,
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "edit-capable delegation has no worktree journal entry".to_string())?;
        if operation.state != DelegationWorktreeOperationState::Applied {
            return Err(format!(
                "delegation worktree is {:?}, not applied",
                operation.state
            ));
        }
    }

    let now = now_iso();
    let mut updated = delegation;
    updated.status = DelegationStatus::Completed;
    updated.updated_at = now.clone();
    if input.summary.is_some() {
        updated.result_summary = input.summary.clone();
    }
    DelegationRepo::save_delegation(&*state, &updated)
        .await
        .map_err(|error| error.to_string())?;
    append_and_publish_parent_event(
        &state,
        &updated.parent_session_id,
        SessionEventKind::DelegationCompleted {
            delegation_id: updated.id.clone(),
            summary: updated.result_summary.clone(),
        },
        CoreEvent::SessionDelegationCompleted {
            session_id: updated.parent_session_id.0.clone(),
            delegation_id: updated.id.0.clone(),
            summary: updated.result_summary.clone(),
        },
        now,
    )
    .await?;

    Ok(ApproveDelegationOutput {
        delegation: updated,
    })
}

#[tauri::command]
pub async fn fail_delegation(
    state: State<'_, SessionCommandState>,
    _app: AppHandle,
    input: FailDelegationInput,
) -> Result<FailDelegationOutput, String> {
    let delegation = DelegationRepo::get_delegation(&*state, &input.delegation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    if is_terminal_status(&delegation.status) {
        return Ok(FailDelegationOutput { delegation });
    }

    let now = now_iso();
    let updated = DelegationRepo::update_delegation_status(
        &*state,
        &input.delegation_id,
        DelegationStatus::Failed,
        now.clone(),
    )
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("delegation not found: {}", input.delegation_id.0))?;
    let reason = input.reason;
    append_and_publish_parent_event(
        &state,
        &updated.parent_session_id,
        SessionEventKind::DelegationFailed {
            delegation_id: updated.id.clone(),
            reason: reason.clone(),
        },
        CoreEvent::SessionDelegationFailed {
            session_id: updated.parent_session_id.0.clone(),
            delegation_id: updated.id.0.clone(),
            reason,
        },
        now,
    )
    .await?;

    Ok(FailDelegationOutput {
        delegation: updated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::{
        project::ProjectId,
        session::{Session, SessionState},
        workspace::{Workspace, WorkspaceState},
    };
    use dcc_core::ports::WorkspaceRepo;
    use dcc_infra::db::SqliteWorkspaceRepo;

    fn child_session(provider_id: &str) -> Session {
        Session {
            id: SessionId("child".to_string()),
            project_id: ProjectId("project".to_string()),
            workspace_id: WorkspaceId("workspace".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: provider_id.to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            updated_at: "2026-09-01T00:00:00Z".to_string(),
        }
    }

    fn legacy_delegation(
        id: &str,
        parent_session_id: SessionId,
        child_session_id: Option<SessionId>,
        target_provider_id: &str,
    ) -> Delegation {
        Delegation {
            id: DelegationId(id.to_string()),
            parent_session_id,
            parent_turn_id: None,
            child_session_id,
            workspace_id: WorkspaceId("workspace".to_string()),
            target_provider_id: ProviderId(target_provider_id.to_string()),
            target_model_id: None,
            mode: DelegationMode::Review,
            status: DelegationStatus::Draft,
            prompt: "legacy delegation".to_string(),
            context_policy: DelegationContextPolicy::Minimal,
            budget: DelegationBudget::default(),
            result_summary: None,
            touched_files: Vec::new(),
            diff_summary: None,
            validation_summary: None,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            updated_at: "2026-09-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn child_session_provider_must_match_the_delegation_target() {
        let child = child_session("codex");
        assert!(validate_child_provider_target(&child, &ProviderId("codex".to_string())).is_ok());
        assert!(validate_child_provider_target(&child, &ProviderId("droid".to_string())).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn legacy_delegation_preflight_rejects_before_status_or_parent_history_changes() {
        let root = tempfile::tempdir().expect("state root");
        let root = std::fs::canonicalize(root.path()).expect("physical state root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let workspace = Workspace {
            id: WorkspaceId("workspace".to_string()),
            project_id: ProjectId("project".to_string()),
            name: None,
            root_path: "/workspace".to_string(),
            base_branch: "main".to_string(),
            worktree_path: None,
            source: None,
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            created_at: "2026-09-01T00:00:00Z".to_string(),
            updated_at: "2026-09-01T00:00:00Z".to_string(),
        };
        SqliteWorkspaceRepo::open(root.join("state.sqlite"))
            .expect("workspace repo")
            .save_workspace(&workspace)
            .await
            .expect("save workspace");
        let mut parent = child_session("codex");
        parent.id = SessionId("parent".to_string());
        SessionRepo::save_session(&state, &parent)
            .await
            .expect("save parent");

        let unknown = legacy_delegation("legacy-unknown", parent.id.clone(), None, "unknown");
        DelegationRepo::save_delegation(&state, &unknown)
            .await
            .expect("save legacy delegation");
        assert!(validate_stored_delegation_target(&state, &unknown)
            .await
            .is_err());
        assert_eq!(
            DelegationRepo::get_delegation(&state, &unknown.id)
                .await
                .expect("load legacy delegation")
                .expect("legacy delegation")
                .status,
            DelegationStatus::Draft
        );
        assert!(SessionEventRepo::list_events_by_session(&state, &parent.id)
            .await
            .expect("parent history")
            .is_empty());

        let other_workspace = Workspace {
            id: WorkspaceId("other-workspace".to_string()),
            ..workspace.clone()
        };
        SqliteWorkspaceRepo::open(root.join("state.sqlite"))
            .expect("workspace repo")
            .save_workspace(&other_workspace)
            .await
            .expect("save other workspace");
        let mut parent_workspace_mismatch =
            legacy_delegation("legacy-parent-workspace", parent.id.clone(), None, "codex");
        parent_workspace_mismatch.workspace_id = other_workspace.id.clone();
        DelegationRepo::save_delegation(&state, &parent_workspace_mismatch)
            .await
            .expect("save parent workspace mismatch");
        assert!(
            validate_stored_delegation_target(&state, &parent_workspace_mismatch)
                .await
                .is_err()
        );
        assert_eq!(
            DelegationRepo::get_delegation(&state, &parent_workspace_mismatch.id)
                .await
                .expect("load parent workspace mismatch")
                .expect("parent workspace mismatch")
                .status,
            DelegationStatus::Draft
        );

        let mut child = child_session("droid");
        child.id = SessionId("legacy-child".to_string());
        SessionRepo::save_session(&state, &child)
            .await
            .expect("save child");
        let mismatch = legacy_delegation(
            "legacy-child-mismatch",
            parent.id.clone(),
            Some(child.id.clone()),
            "codex",
        );
        DelegationRepo::save_delegation(&state, &mismatch)
            .await
            .expect("save mismatch delegation");
        assert!(validate_stored_delegation_target(&state, &mismatch)
            .await
            .is_err());
        assert_eq!(
            DelegationRepo::get_delegation(&state, &mismatch.id)
                .await
                .expect("load mismatch delegation")
                .expect("mismatch delegation")
                .status,
            DelegationStatus::Draft
        );
        assert!(SessionEventRepo::list_events_by_session(&state, &parent.id)
            .await
            .expect("parent history")
            .is_empty());

        let mut wrong_workspace_child = child_session("droid");
        wrong_workspace_child.id = SessionId("legacy-child-workspace".to_string());
        wrong_workspace_child.workspace_id = other_workspace.id.clone();
        SessionRepo::save_session(&state, &wrong_workspace_child)
            .await
            .expect("save child in another workspace");
        let child_workspace_mismatch = legacy_delegation(
            "legacy-child-workspace-mismatch",
            parent.id.clone(),
            Some(wrong_workspace_child.id.clone()),
            "droid",
        );
        DelegationRepo::save_delegation(&state, &child_workspace_mismatch)
            .await
            .expect("save child workspace mismatch");
        assert!(
            validate_stored_delegation_target(&state, &child_workspace_mismatch)
                .await
                .is_err()
        );
        assert_eq!(
            DelegationRepo::get_delegation(&state, &child_workspace_mismatch.id)
                .await
                .expect("load child workspace mismatch")
                .expect("child workspace mismatch")
                .status,
            DelegationStatus::Draft
        );
        assert!(SessionEventRepo::list_events_by_session(&state, &parent.id)
            .await
            .expect("parent history")
            .is_empty());
    }
}
