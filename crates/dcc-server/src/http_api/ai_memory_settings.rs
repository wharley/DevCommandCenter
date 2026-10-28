//! Settings → AI Memory for the native app: the sidecar's settings (the
//! token in the keychain), the sessions waiting to be exported with "Tentar
//! novamente", what was exported, and restarting DCC when a change needs it.
use super::*;
use crate::ai_memory_sidecar::{AiMemorySettingsInput, AiMemorySidecar};
use dcc_tauri::commands::session_commands as commands;

#[derive(Deserialize)]
pub(super) struct LimitQuery {
    limit: Option<usize>,
}

pub(super) async fn settings(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let settings = AiMemorySidecar::read_settings(backend.sessions.app_data_dir())
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(settings)))
}

pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<AiMemorySettingsInput>,
) -> Result<Json<Value>, HttpApiError> {
    let settings = AiMemorySidecar::save_settings(backend.sessions.app_data_dir(), input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(settings)))
}

pub(super) async fn outbox(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let entries = commands::ai_memory_outbox_list_for_state(
        &backend.sessions,
        Some(query.limit.unwrap_or(50).min(500)),
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(entries)))
}

pub(super) async fn retry(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let entry = commands::ai_memory_outbox_retry_for_state(&backend.sessions, &session_id)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(entry)))
}

pub(super) async fn history(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let entries = commands::ai_memory_export_history_for_state(
        &backend.sessions,
        Some(query.limit.unwrap_or(100).min(500)),
    )
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(entries)))
}

/// `restart_dcc`: the desktop restarts itself (the native app reconnects).
pub(super) async fn restart(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let app = backend
        .app
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::not_found("the desktop app is not running"))?;
    // After the answer leaves: restarting ends this process.
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        app.restart();
    });
    Ok(Json(json!({ "ok": true })))
}
