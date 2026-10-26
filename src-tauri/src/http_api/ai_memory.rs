//! What the native conversation header shows of ai-memory: whether a
//! session still waits to be exported, what the agent received on its
//! latest turn (kept in this process), the person's curation of those
//! sources (pin, correct, ignore, clear) and the sidecar's mode. They run
//! the desktop's own commands.
use super::*;
use crate::ai_memory_sidecar::AiMemorySidecar;
use dcc_tauri::commands::session_commands::{self as commands, AiMemorySourceActionInput};

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

/// `ai_memory_export_status`: `null` when nothing waits (the steady state).
pub(super) async fn export_status(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::ai_memory_export_status_with_state(
        &backend.sessions,
        session_id,
    ))
}

/// `ai_memory_recovered_sources`: the sources of the session's latest turn.
pub(super) async fn sources(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::ai_memory_recovered_sources_with_state(
        &backend.sessions,
        session_id,
    ))
}

/// `ai_memory_source_actions`: the person's curation, newest first.
pub(super) async fn source_actions(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::ai_memory_source_actions_with_state(
        &backend.sessions,
        Some(200),
    ))
}

/// `ai_memory_source_action_save`: pinned, ignored, corrected or cleared.
pub(super) async fn save_source_action(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<AiMemorySourceActionInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(commands::ai_memory_source_action_save_with_state(
        &backend.sessions,
        input,
    ))
}

/// `ai_memory_sidecar_status`; `disabled` without the desktop's sidecar.
pub(super) async fn sidecar_status(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    use tauri::Manager;
    let status = backend
        .app
        .get()
        .and_then(|app| app.try_state::<AiMemorySidecar>())
        .map(|sidecar| sidecar.status());
    Ok(Json(match status {
        Some(status) => serde_json::to_value(status)
            .map_err(|error| HttpApiError::internal(error.to_string()))?,
        None => json!({ "mode": "disabled", "running": false }),
    }))
}
