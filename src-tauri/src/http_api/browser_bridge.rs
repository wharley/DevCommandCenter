//! The native app's Browser driven by the desktop's agent tools. The agents
//! run here and call `dcc_browser_*`; when the native app's Browser is the
//! one on screen, every page operation (a script, a navigation, a reload)
//! is streamed to it and its answer comes back by `opId`. The native app
//! also reports its Browser's lifecycle (opened, loading, loaded, its URL
//! and title, visibility, closed) so the desktop's own bookkeeping — scope,
//! lifecycle token, page-load revision, the grant, semantic maps, evidence,
//! audit — keeps working unchanged. Local clients only: the relay refuses
//! `/api/v1/browser/bridge/`.
use super::*;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context as TaskContext, Poll};
use tokio::sync::{broadcast, oneshot};

/// One page operation for the native Browser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BridgeOpKind {
    /// Runs a script; its result's JSON comes back.
    Eval {
        script: String,
    },
    /// Runs a script; nothing comes back.
    EvalDetached {
        script: String,
    },
    Navigate {
        url: String,
    },
    Reload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeOp {
    pub op_id: u64,
    #[serde(flatten)]
    pub kind: BridgeOpKind,
}

/// The native Browser's answer to an operation.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeReply {
    pub ok: bool,
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// What the native Browser reports about its page.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PageReport {
    /// A main-frame navigation started.
    Loading {
        url: String,
    },
    /// The page finished loading at this URL.
    Loaded {
        url: String,
    },
    /// The URL moved without a load (a single-page app).
    Url {
        url: String,
    },
    Title {
        title: String,
    },
    /// Shown, or hidden under an overlay or a drag.
    Visibility {
        visible: bool,
    },
}

const OP_CAPACITY: usize = 64;

pub struct NativeBrowserBridge {
    ops: broadcast::Sender<BridgeOp>,
    pending: Mutex<HashMap<u64, oneshot::Sender<BridgeReply>>>,
    next: AtomicU64,
}

/// The one bridge of this process.
pub fn native_browser_bridge() -> &'static NativeBrowserBridge {
    static BRIDGE: OnceLock<NativeBrowserBridge> = OnceLock::new();
    BRIDGE.get_or_init(|| NativeBrowserBridge {
        ops: broadcast::channel(OP_CAPACITY).0,
        pending: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
    })
}

impl NativeBrowserBridge {
    /// The native app is listening.
    pub fn connected(&self) -> bool {
        self.ops.receiver_count() > 0
    }

    fn send(&self, kind: BridgeOpKind) -> Result<u64, String> {
        if !self.connected() {
            return Err("browser is not open".to_string());
        }
        let op_id = self.next.fetch_add(1, Ordering::Relaxed);
        self.ops
            .send(BridgeOp { op_id, kind })
            .map_err(|_| "browser is not open".to_string())?;
        Ok(op_id)
    }

    /// An operation whose answer is awaited (the caller bounds the wait).
    pub fn request(&self, kind: BridgeOpKind) -> Result<oneshot::Receiver<BridgeReply>, String> {
        let (sender, receiver) = oneshot::channel();
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "browser bridge lock poisoned".to_string())?;
        // Answers that never came (timed out, page gone) leave no trace.
        pending.retain(|_, waiting| !waiting.is_closed());
        let op_id = self.send(kind)?;
        pending.insert(op_id, sender);
        Ok(receiver)
    }

    /// An operation nothing waits for.
    pub fn fire(&self, kind: BridgeOpKind) -> Result<(), String> {
        self.send(kind).map(|_| ())
    }

    /// Whether someone still waited for this answer.
    pub fn reply(&self, op_id: u64, reply: BridgeReply) -> bool {
        let waiting = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&op_id));
        waiting.is_some_and(|waiting| waiting.send(reply).is_ok())
    }

    /// The operations as they go out (the stream; tests standing in for
    /// the native app).
    pub fn subscribe(&self) -> broadcast::Receiver<BridgeOp> {
        self.ops.subscribe()
    }
}

pub type BrowserFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

/// What the desktop's Browser state offers the native app's Browser.
pub trait DesktopBrowser: Send + Sync {
    /// The native Browser opened on `url` for a task's conversation: it is
    /// the desktop's Browser surface now. Answers its lifecycle token.
    fn open(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        url: String,
    ) -> BrowserFuture<u64>;
    fn report(&self, lifecycle_token: u64, report: PageReport) -> BrowserFuture<()>;
    fn close(&self, lifecycle_token: u64) -> BrowserFuture<()>;
    /// The native app stopped listening (it quit or crashed).
    fn disconnected(&self) -> BrowserFuture<()>;
    /// Arms (`Some(true)`), disarms (`Some(false)`) or reads the agent's
    /// control: `{ armed, remainingMs }`.
    fn control(
        &self,
        workspace_id: String,
        session_id: Option<String>,
        lifecycle_token: u64,
        armed: Option<bool>,
    ) -> BrowserFuture<Value>;
}

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/browser/bridge/stream", get(stream))
        .route("/api/v1/browser/bridge/ops/:op_id/reply", post(reply))
        .route("/api/v1/browser/bridge/surface", post(open))
        .route("/api/v1/browser/bridge/surface/:token/report", post(report))
        .route("/api/v1/browser/bridge/surface/:token/close", post(close))
        .route("/api/v1/browser/bridge/control", post(control))
}

fn host(backend: &HttpBackend) -> Result<Arc<dyn DesktopBrowser>, HttpApiError> {
    backend
        .desktop_browser
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::internal("the desktop's Browser is not attached"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenInput {
    workspace_id: String,
    #[serde(default)]
    session_id: Option<String>,
    url: String,
}

async fn open(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<OpenInput>,
) -> Result<Json<Value>, HttpApiError> {
    let token = host(&backend)?
        .open(input.workspace_id, input.session_id, input.url)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "lifecycleToken": token })))
}

async fn report(
    State(backend): State<Arc<HttpBackend>>,
    Path(token): Path<u64>,
    Json(input): Json<PageReport>,
) -> Result<Json<Value>, HttpApiError> {
    host(&backend)?
        .report(token, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

async fn close(
    State(backend): State<Arc<HttpBackend>>,
    Path(token): Path<u64>,
) -> Result<Json<Value>, HttpApiError> {
    host(&backend)?
        .close(token)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ControlInput {
    workspace_id: String,
    #[serde(default)]
    session_id: Option<String>,
    lifecycle_token: u64,
    #[serde(default)]
    armed: Option<bool>,
}

async fn control(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ControlInput>,
) -> Result<Json<Value>, HttpApiError> {
    let status = host(&backend)?
        .control(
            input.workspace_id,
            input.session_id,
            input.lifecycle_token,
            input.armed,
        )
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(status))
}

async fn reply(Path(op_id): Path<u64>, Json(input): Json<BridgeReply>) -> Json<Value> {
    Json(json!({ "delivered": native_browser_bridge().reply(op_id, input) }))
}

/// Tells the desktop when the native app's listening ends.
struct Disconnect(Option<Arc<dyn DesktopBrowser>>);

impl Drop for Disconnect {
    fn drop(&mut self) {
        if let Some(host) = self.0.take() {
            tokio::spawn(async move {
                // The receiver goes with this stream; a new one may come.
                tokio::time::sleep(Duration::from_millis(200)).await;
                if !native_browser_bridge().connected() {
                    let _ = host.disconnected().await;
                }
            });
        }
    }
}

/// The operations, for as long as the native app listens.
struct OpStream<S> {
    inner: Pin<Box<S>>,
    _disconnect: Disconnect,
}

impl<S: tokio_stream::Stream> tokio_stream::Stream for OpStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

async fn stream(
    State(backend): State<Arc<HttpBackend>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let ops = BroadcastStream::new(native_browser_bridge().subscribe()).filter_map(|message| {
        match message {
            Ok(op) => serde_json::to_string(&op)
                .ok()
                .map(|payload| Ok(Event::default().event("browser-op").data(payload))),
            // An operation it missed fails on its own timeout.
            Err(_) => Some(Ok(Event::default().event("browser-lagged").data("{}"))),
        }
    });
    let stream = OpStream {
        inner: Box::pin(ops),
        _disconnect: Disconnect(backend.desktop_browser.get().cloned()),
    };
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_and_reports_read_like_the_native_app_expects() {
        let op = BridgeOp {
            op_id: 7,
            kind: BridgeOpKind::Eval { script: "1".into() },
        };
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            json!({ "opId": 7, "kind": "eval", "script": "1" })
        );
        let reload = BridgeOp {
            op_id: 8,
            kind: BridgeOpKind::Reload,
        };
        assert_eq!(
            serde_json::to_value(&reload).unwrap(),
            json!({ "opId": 8, "kind": "reload" })
        );
        let report: PageReport =
            serde_json::from_value(json!({ "kind": "loaded", "url": "https://ex.com/" })).unwrap();
        assert_eq!(
            report,
            PageReport::Loaded {
                url: "https://ex.com/".into()
            }
        );
    }

    #[tokio::test]
    async fn an_answer_reaches_who_asked_and_nobody_listening_is_closed() {
        let bridge = NativeBrowserBridge {
            ops: broadcast::channel(4).0,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        };
        assert_eq!(
            bridge.request(BridgeOpKind::Reload).unwrap_err(),
            "browser is not open"
        );
        let mut listener = bridge.subscribe();
        let answer = bridge
            .request(BridgeOpKind::Eval { script: "2".into() })
            .unwrap();
        let op = listener.recv().await.unwrap();
        assert!(bridge.reply(
            op.op_id,
            BridgeReply {
                ok: true,
                raw: Some("2".into()),
                error: None,
            }
        ));
        assert_eq!(answer.await.unwrap().raw.as_deref(), Some("2"));
        assert!(!bridge.reply(op.op_id, BridgeReply::default()), "only once");
    }
}
