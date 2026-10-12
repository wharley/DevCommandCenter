//! Delivering a task from the phone: diff summary, commit (with the
//! desktop's deterministic message, editable), push, "Atualizar e enviar",
//! PR (draft or not), PR/CI state, complete and archive.
//!
//! Everything runs through `dcc_tauri::commands::workspace_delivery`, the
//! desktop's own delivery functions in the desktop's order. The task is
//! named by id; its folder comes from the database, never from the phone.
//! Destructive steps (push, archive) are confirmed in the app.
use super::*;
use dcc_tauri::commands::workspace_delivery::{
    self, DeliveryCommitInput, DeliveryError, DeliveryRequestInput,
};

fn classify(error: DeliveryError) -> HttpApiError {
    let message = error.to_string();
    match error {
        DeliveryError::NotFound(_) => HttpApiError::NotFound(message),
        DeliveryError::Stale => HttpApiError::Refused("stale", message, Value::Null),
        DeliveryError::ChecksFailed => HttpApiError::Refused("checks_failed", message, Value::Null),
        DeliveryError::BehindRemote(target) => {
            HttpApiError::Refused("behind_remote", message, json!({ "target": target }))
        }
        DeliveryError::Invalid(_) => HttpApiError::BadRequest(message),
        DeliveryError::Failed(_) => HttpApiError::Internal(message),
    }
}

fn ok_json<T: Serialize>(value: T) -> Result<Json<Value>, HttpApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

pub(super) async fn summary(
    State(backend): State<Arc<HttpBackend>>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    ok_json(
        workspace_delivery::delivery_summary(&backend.workspace, &workspace_id)
            .await
            .map_err(classify)?,
    )
}

pub(super) async fn forge(
    State(backend): State<Arc<HttpBackend>>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    ok_json(
        workspace_delivery::delivery_forge(&backend.workspace, &workspace_id)
            .await
            .map_err(classify)?,
    )
}

fn body<T: serde::de::DeserializeOwned + Default>(raw: &[u8]) -> Result<T, HttpApiError> {
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(raw).map_err(|error| HttpApiError::BadRequest(error.to_string()))
}

pub(super) async fn act(
    State(backend): State<Arc<HttpBackend>>,
    Path((workspace_id, action)): Path<(String, String)>,
    raw: axum::body::Bytes,
) -> Result<Json<Value>, HttpApiError> {
    let state = &backend.workspace;
    let result = match action.as_str() {
        "commit" => {
            let input: DeliveryCommitInput = body(&raw)?;
            workspace_delivery::delivery_commit(state, &backend.sessions, &workspace_id, input).await
        }
        "push" => workspace_delivery::delivery_push(state, &workspace_id).await,
        "sync-and-push" => workspace_delivery::delivery_sync_and_push(state, &workspace_id).await,
        "request" => {
            let input: DeliveryRequestInput = body(&raw)?;
            workspace_delivery::delivery_create_request(state, &backend.sessions, &workspace_id, input)
                .await
        }
        "complete" => workspace_delivery::delivery_complete(state, &workspace_id).await,
        "archive" => workspace_delivery::delivery_archive(state, &workspace_id).await,
        _ => return Err(HttpApiError::not_found("unknown delivery action")),
    };
    result.map_err(classify)?;
    Ok(Json(json!({ "ok": true })))
}
