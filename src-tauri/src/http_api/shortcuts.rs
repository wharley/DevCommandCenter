//! The desktop's global shortcuts for the native app's Settings: the quick
//! composer's and Appshots' keys (registered by the desktop, which owns
//! those windows), and opening the quick composer. Local only.
use super::*;

/// What the desktop's global shortcuts offer the native app.
pub trait DesktopShortcuts: Send + Sync {
    /// `quick_composer_status`
    fn quick_composer(&self) -> Result<Value, String>;
    fn set_quick_composer(&self, shortcut: Option<String>) -> Result<(), String>;
    /// `quick_composer_toggle`
    fn toggle_quick_composer(&self) -> Result<(), String>;
    /// `appshots_status` without the windows.
    fn appshots(&self) -> Result<Value, String>;
    fn set_appshots(&self, shortcut: Option<String>) -> Result<(), String>;
}

#[derive(Deserialize)]
pub(super) struct ShortcutInput {
    shortcut: Option<String>,
}

fn shortcuts(backend: &HttpBackend) -> Result<&Arc<dyn DesktopShortcuts>, HttpApiError> {
    backend
        .desktop_shortcuts
        .get()
        .ok_or_else(|| HttpApiError::not_found("global shortcuts are not available"))
}

/// The desktop's own error codes (`shortcutInvalid`, `shortcutUnavailable`).
fn refused(code: String) -> HttpApiError {
    HttpApiError::bad_request(code)
}

pub(super) async fn quick_composer(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .quick_composer()
        .map(Json)
        .map_err(refused)
}

pub(super) async fn set_quick_composer(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ShortcutInput>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .set_quick_composer(input.shortcut)
        .map_err(refused)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn toggle_quick_composer(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .toggle_quick_composer()
        .map_err(refused)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn appshots(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?.appshots().map(Json).map_err(refused)
}

pub(super) async fn set_appshots(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ShortcutInput>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .set_appshots(input.shortcut)
        .map_err(refused)?;
    Ok(Json(json!({ "ok": true })))
}
