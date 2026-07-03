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
        provider::ProviderId,
        session::{SessionEventKind, SessionEventRecord, SessionId, TurnId},
        workspace::WorkspaceId,
    },
    ports::{DelegationRepo, SessionEventRepo, SessionRepo},
};

use crate::state::SessionCommandState;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateDelegationInput {
    pub parent_session_id: SessionId,
    pub parent_turn_id: Option<TurnId>,
    pub child_session_id: Option<SessionId>,
    pub workspace_id: WorkspaceId,
    pub target_provider_id: ProviderId,
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

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

async fn append_parent_event(
    state: &SessionCommandState,
    session_id: &SessionId,
    kind: SessionEventKind,
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
    SessionEventRepo::append_event(state, &event)
        .await
        .map_err(|error| error.to_string())
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
    if let Some(child_session_id) = input.child_session_id.as_ref() {
        let child_session = SessionRepo::get_session(&*state, child_session_id)
            .await
            .map_err(|error| error.to_string())?;
        if child_session.is_none() {
            return Err(format!("child session not found: {}", child_session_id.0));
        }
    }

    let now = now_iso();
    let delegation = Delegation {
        id: DelegationId(Uuid::new_v4().to_string()),
        parent_session_id: input.parent_session_id.clone(),
        parent_turn_id: input.parent_turn_id,
        child_session_id: input.child_session_id,
        workspace_id: input.workspace_id,
        target_provider_id: input.target_provider_id,
        mode: input.mode,
        status: DelegationStatus::Draft,
        prompt: input.prompt,
        context_policy: input.context_policy,
        budget: input.budget,
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    DelegationRepo::save_delegation(&*state, &delegation)
        .await
        .map_err(|error| error.to_string())?;
    append_parent_event(
        &state,
        &delegation.parent_session_id,
        SessionEventKind::DelegationRequested {
            delegation_id: delegation.id.clone(),
        },
        now,
    )
    .await?;

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
    if matches!(
        delegation.status,
        DelegationStatus::Completed | DelegationStatus::Failed | DelegationStatus::Cancelled
    ) {
        return Err(format!(
            "delegation {} is already {}",
            delegation.id.0,
            match delegation.status {
                DelegationStatus::Completed => "completed",
                DelegationStatus::Failed => "failed",
                DelegationStatus::Cancelled => "cancelled",
                _ => "terminal",
            }
        ));
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
    append_parent_event(
        &state,
        &updated.parent_session_id,
        SessionEventKind::DelegationCancelled {
            delegation_id: updated.id.clone(),
            reason: input.reason,
        },
        now,
    )
    .await?;

    Ok(CancelDelegationOutput {
        delegation: updated,
    })
}
