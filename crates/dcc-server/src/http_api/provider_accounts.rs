//! Settings → Providers → Accounts for the native app: the desktop keeps
//! the named sign-ins (each with its own config folder) and asks each
//! runtime who it is signed in as; signing in waits on the browser.
use super::*;
use dcc_tauri::commands::provider_commands::{
    cancel_account_sign_in, provider_account_status_for_state, sign_in_provider_account_for_state,
    ProviderAccountStatusInput,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateInput {
    provider_id: String,
    label: String,
}

#[derive(Deserialize)]
pub(super) struct RenameInput {
    label: String,
}

fn failed(error: impl ToString) -> HttpApiError {
    HttpApiError::bad_request(error.to_string())
}

pub(super) async fn list(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let accounts = backend.sessions.list_provider_accounts().map_err(failed)?;
    Ok(Json(json!({ "accounts": accounts })))
}

pub(super) async fn create(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<CreateInput>,
) -> Result<Json<Value>, HttpApiError> {
    let account = backend
        .sessions
        .create_provider_account(input.provider_id.trim(), &input.label, None)
        .map_err(failed)?;
    Ok(Json(json!({ "account": account })))
}

pub(super) async fn rename(
    State(backend): State<Arc<HttpBackend>>,
    Path(account_id): Path<String>,
    Json(input): Json<RenameInput>,
) -> Result<Json<Value>, HttpApiError> {
    let account = backend
        .sessions
        .rename_provider_account(account_id.trim(), &input.label)
        .map_err(failed)?;
    Ok(Json(json!({ "account": account })))
}

pub(super) async fn remove(
    State(backend): State<Arc<HttpBackend>>,
    Path(account_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .remove_provider_account(account_id.trim())
        .map_err(failed)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn status(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ProviderAccountStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = provider_account_status_for_state(&backend.sessions, input)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}

/// Returns once the person authorized, cancelled ("sign-in cancelled") or
/// the sign-in gave up.
pub(super) async fn sign_in(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ProviderAccountStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = sign_in_provider_account_for_state(&backend.sessions, input)
        .await
        .map_err(failed)?;
    Ok(Json(json!(output)))
}

pub(super) async fn cancel_sign_in(
    Json(input): Json<ProviderAccountStatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    cancel_account_sign_in(&input).map_err(failed)?;
    Ok(Json(json!({ "ok": true })))
}
