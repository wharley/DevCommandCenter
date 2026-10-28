//! The menu bar's snapshot for the native app's own menu bar item: the
//! tasks that are running, waiting or just finished, the DCC processes'
//! CPU and memory, and the keep-awake hold, as the desktop reads them.
//! Local only (the relay refuses it).
use super::*;

/// What the desktop's menu bar offers the native app.
pub trait DesktopMenuBar: Send + Sync {
    /// `menu_bar_snapshot`, already serialized.
    fn snapshot(&self) -> Result<Value, String>;
}

pub(super) async fn snapshot(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let menu_bar = backend
        .desktop_menu_bar
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::not_found("the menu bar is not available"))?;
    let read = tokio::task::spawn_blocking(move || menu_bar.snapshot())
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?;
    read.map(Json).map_err(HttpApiError::internal)
}
