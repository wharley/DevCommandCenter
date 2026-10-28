//! The desktop's global shortcuts for the native app's Settings: the quick
//! composer's and Appshots' keys (registered by the desktop, which owns
//! those windows), and opening the quick composer. On this branch the
//! native app draws the quick composer: while it listens on
//! `GET /api/v1/quick-composer/stream`, the key (and "Abrir") reach it
//! there, and its launches keep the desktop's journal (`begin`,
//! `checkpoint`). Local only.
use super::*;
use std::sync::OnceLock;
use tokio::sync::broadcast;

/// The native app's quick composer, while it listens.
pub struct NativeQuickComposer {
    toggles: broadcast::Sender<()>,
}

pub fn native_quick_composer() -> &'static NativeQuickComposer {
    static QUICK: OnceLock<NativeQuickComposer> = OnceLock::new();
    QUICK.get_or_init(|| NativeQuickComposer {
        toggles: broadcast::channel(8).0,
    })
}

impl NativeQuickComposer {
    /// Shows or hides the native app's quick composer; `false` when it is
    /// not listening.
    pub fn toggle(&self) -> bool {
        self.toggles.send(()).is_ok()
    }
}

/// What the desktop's global shortcuts offer the native app.
pub trait DesktopShortcuts: Send + Sync {
    /// `quick_composer_status`
    fn quick_composer(&self) -> Result<Value, String>;
    fn set_quick_composer(&self, shortcut: Option<String>) -> Result<(), String>;
    /// `quick_composer_toggle`
    fn toggle_quick_composer(&self) -> Result<(), String>;
    /// `quick_composer_begin` / `quick_composer_checkpoint`.
    fn begin_quick_launch(&self, launch: Value) -> Result<Value, String>;
    fn checkpoint_quick_launch(&self, launch: Value) -> Result<Value, String>;
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

pub(super) async fn begin_quick_launch(
    State(backend): State<Arc<HttpBackend>>,
    Json(launch): Json<Value>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .begin_quick_launch(launch)
        .map(Json)
        .map_err(refused)
}

pub(super) async fn checkpoint_quick_launch(
    State(backend): State<Arc<HttpBackend>>,
    Json(launch): Json<Value>,
) -> Result<Json<Value>, HttpApiError> {
    shortcuts(&backend)?
        .checkpoint_quick_launch(launch)
        .map(Json)
        .map_err(refused)
}

/// One `toggle` frame per press of the key (or "Abrir").
pub(super) async fn quick_composer_stream(
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let toggles = native_quick_composer().toggles.subscribe();
    let frames = futures_util::stream::unfold(toggles, |mut toggles| async move {
        loop {
            match toggles.recv().await {
                Ok(()) => {
                    let event = Event::default().event("toggle").data("{}");
                    return Some((Ok(event), toggles));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(frames).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_key_reaches_the_native_app_only_while_it_listens() {
        let quick = NativeQuickComposer {
            toggles: broadcast::channel(4).0,
        };
        assert!(!quick.toggle(), "nobody listening");
        let mut listening = quick.toggles.subscribe();
        assert!(quick.toggle());
        assert!(listening.recv().await.is_ok());
    }
}
