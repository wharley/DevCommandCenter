//! Settings → Decision Provider for the native app: the desktop keeps the
//! settings (the key in the keychain) and applies them to its own process;
//! the history is the decisions it recorded.
use super::*;
use crate::decision_provider_settings::{DecisionProviderSettings, DecisionProviderSettingsInput};

#[derive(Deserialize)]
pub(super) struct HistoryQuery {
    limit: Option<usize>,
}

pub(super) async fn settings(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let settings = DecisionProviderSettings::read_settings(backend.sessions.app_data_dir())
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(settings)))
}

pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<DecisionProviderSettingsInput>,
) -> Result<Json<Value>, HttpApiError> {
    let settings = DecisionProviderSettings::save_settings(backend.sessions.app_data_dir(), input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(settings)))
}

pub(super) async fn history(
    State(backend): State<Arc<HttpBackend>>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Value>, HttpApiError> {
    let entries = dcc_tauri::commands::session_commands::decision_provider_history(
        &backend.sessions,
        query.limit.unwrap_or(100).min(500),
    )
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(entries)))
}
