//! "Manter o Mac acordado" for the native app's Settings: the desktop owns
//! the hold (the power assertion and the menu bar), the native app reads
//! and sets its mode here. No desktop controller yet: 404.
use super::*;
use crate::keep_awake::{KeepAwakeMode, KeepAwakeStatus};

/// What the desktop's keep-awake controller offers the native app.
pub trait DesktopKeepAwake: Send + Sync {
    fn status(&self) -> Option<KeepAwakeStatus>;
    fn set_mode(&self, mode: KeepAwakeMode) -> Option<Result<KeepAwakeStatus, String>>;
}

#[derive(Deserialize)]
pub(super) struct ModeInput {
    mode: KeepAwakeMode,
}

fn controller(backend: &HttpBackend) -> Result<&Arc<dyn DesktopKeepAwake>, HttpApiError> {
    backend
        .desktop_keep_awake
        .get()
        .ok_or_else(|| HttpApiError::not_found("keep-awake is not available"))
}

pub(super) async fn status(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let status = controller(&backend)?
        .status()
        .ok_or_else(|| HttpApiError::not_found("keep-awake is not available"))?;
    Ok(Json(json!(status)))
}

pub(super) async fn set_mode(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ModeInput>,
) -> Result<Json<Value>, HttpApiError> {
    let status = controller(&backend)?
        .set_mode(input.mode)
        .ok_or_else(|| HttpApiError::not_found("keep-awake is not available"))?
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(status)))
}
