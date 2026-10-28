//! Settings → Providers for the native app: how much of each sign-in's plan
//! is left (and spending a saved reset), and Antigravity's managed runtime
//! and Google sign-in. The desktop runs each provider's CLI to answer.
use super::*;
use dcc_tauri::commands::provider_commands::{
    antigravity_status_for_state, connect_antigravity_for_state, install_antigravity_for_state,
    provider_account_reset_for_state, provider_account_usage_for_state, AntigravityStatusInput,
    ConnectAntigravityInput, ProviderAccountResetInput, ProviderAccountUsageInput,
};

fn failed(error: String) -> HttpApiError {
    HttpApiError::bad_request(error)
}

pub(super) async fn usage(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ProviderAccountUsageInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = provider_account_usage_for_state(&backend.sessions, input)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}

pub(super) async fn reset(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ProviderAccountResetInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = provider_account_reset_for_state(&backend.sessions, input)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}

pub(super) async fn antigravity_status(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<AntigravityStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = antigravity_status_for_state(&backend.sessions, input).map_err(failed)?;
    Ok(Json(json!(output)))
}

pub(super) async fn install_antigravity(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let output = install_antigravity_for_state(&backend.sessions)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}

pub(super) async fn connect_antigravity(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ConnectAntigravityInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = connect_antigravity_for_state(&backend.sessions, input)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}
