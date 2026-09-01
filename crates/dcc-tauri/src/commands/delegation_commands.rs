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
