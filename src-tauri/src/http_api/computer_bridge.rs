//! Computer use for the native app (local clients only, like the Browser's
//! bridge). While the native app listens on
//! `GET /api/v1/computer/bridge/stream` it owns the agents' control
//! requests: it gets the ones waiting for the person (`control-requests`,
//! again whenever they change), reads the Mac's state for one (`status`:
//! the permissions, the conversation's runtime, the apps with a window),
//! opens a permission's system settings (`access`) and answers (`respond`,
//! first answer wins, shared with the phone). The desktop's own dialog
//! stays quiet meanwhile. The stream also carries what the agent does
//! (`computer-activity`) and when the preview's frame changes, for the
//! native app's preview, which reads the frame from `preview`. The desktop still does the controlling: the
//! permissions are its own.
use super::browser_bridge::BrowserFuture;
use super::*;
use tokio::sync::broadcast;

const POLL: Duration = Duration::from_millis(500);

/// What the desktop's computer use offers the native app.
pub trait DesktopComputer: Send + Sync {
    /// The control requests waiting (`PendingComputerControlRequest`s).
    fn pending(&self) -> Value;
    /// `computer_use_status` (`ComputerUseStatus`).
    fn status(&self, session_id: Option<String>, include_targets: bool) -> BrowserFuture<Value>;
    /// `computer_use_request_access`: `accessibility` or `screenRecording`.
    fn request_access(&self, kind: String) -> BrowserFuture<Value>;
    /// The session's latest preview frame (`ComputerUsePreviewFrame` or
    /// null); `refresh` captures a new one first.
    fn preview(&self, session_id: String, refresh: bool) -> BrowserFuture<Value>;
    /// `computer_use_respond_control_request`
    fn respond(
        &self,
        request_id: String,
        allowed: bool,
        allowed_bundle_ids: Vec<String>,
    ) -> BrowserFuture<Value>;
}

pub struct NativeComputer {
    /// The stream's listeners; computer use's live events go out here too.
    events: broadcast::Sender<(String, Value)>,
}

pub fn native_computer() -> &'static NativeComputer {
    static COMPUTER: OnceLock<NativeComputer> = OnceLock::new();
    COMPUTER.get_or_init(|| NativeComputer {
        events: broadcast::channel(64).0,
    })
}

impl NativeComputer {
    /// Whether the native app answers computer use now.
    pub fn connected(&self) -> bool {
        self.events.receiver_count() > 0
    }

    /// A live event for the native app's preview (`computer-activity`,
    /// `computer-preview-updated`, `computer-preview-cleared`); dropped
    /// when it is not listening.
    pub fn publish(&self, name: &str, data: Value) {
        if self.connected() {
            let _ = self.events.send((name.to_string(), data));
        }
    }
}

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/computer/bridge/stream", get(stream))
        .route("/api/v1/computer/bridge/status", post(status))
        .route("/api/v1/computer/bridge/access", post(access))
        .route("/api/v1/computer/bridge/preview", post(preview))
        .route(
            "/api/v1/computer/bridge/requests/:id/respond",
            post(respond),
        )
}

fn host(backend: &HttpBackend) -> Result<Arc<dyn DesktopComputer>, HttpApiError> {
    backend
        .desktop_computer
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::internal("the desktop's computer use is not attached"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusInput {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    include_targets: bool,
}

async fn status(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<StatusInput>,
) -> Result<Json<Value>, HttpApiError> {
    let status = host(&backend)?
        .status(input.session_id, input.include_targets)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(status))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewInput {
    session_id: String,
    #[serde(default)]
    refresh: bool,
}

async fn preview(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PreviewInput>,
) -> Result<Json<Value>, HttpApiError> {
    let frame = host(&backend)?
        .preview(input.session_id, input.refresh)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "frame": frame })))
}

#[derive(Debug, Deserialize)]
struct AccessInput {
    kind: String,
}

async fn access(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<AccessInput>,
) -> Result<Json<Value>, HttpApiError> {
    let status = host(&backend)?
        .request_access(input.kind)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(status))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RespondInput {
    allowed: bool,
    #[serde(default)]
    allowed_bundle_ids: Vec<String>,
}

async fn respond(
    State(backend): State<Arc<HttpBackend>>,
    Path(id): Path<String>,
    Json(input): Json<RespondInput>,
) -> Result<Json<Value>, HttpApiError> {
    let output = host(&backend)?
        .respond(id, input.allowed, input.allowed_bundle_ids)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(output))
}

struct Watch {
    host: Option<Arc<dyn DesktopComputer>>,
    events: broadcast::Receiver<(String, Value)>,
    tick: tokio::time::Interval,
    last: Option<String>,
}

async fn next_frame(mut watch: Watch) -> Option<(Result<Event, std::convert::Infallible>, Watch)> {
    loop {
        tokio::select! {
            event = watch.events.recv() => match event {
                Ok((name, data)) => {
                    let event = Event::default().event(name).data(data.to_string());
                    return Some((Ok(event), watch));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            },
            _ = watch.tick.tick() => {
                let pending = watch
                    .host
                    .as_ref()
                    .map(|host| host.pending())
                    .unwrap_or_else(|| json!([]))
                    .to_string();
                if watch.last.as_deref() != Some(pending.as_str()) {
                    watch.last = Some(pending.clone());
                    let event = Event::default().event("control-requests").data(pending);
                    return Some((Ok(event), watch));
                }
            }
        }
    }
}

async fn stream(
    State(backend): State<Arc<HttpBackend>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut tick = tokio::time::interval(POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let watch = Watch {
        host: backend.desktop_computer.get().cloned(),
        events: native_computer().events.subscribe(),
        tick,
        last: None,
    };
    Sse::new(futures_util::stream::unfold(watch, next_frame)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Value);

    impl DesktopComputer for Fixed {
        fn pending(&self) -> Value {
            self.0.clone()
        }
        fn status(&self, _: Option<String>, _: bool) -> BrowserFuture<Value> {
            Box::pin(async { Ok(json!({})) })
        }
        fn request_access(&self, _: String) -> BrowserFuture<Value> {
            Box::pin(async { Ok(json!({})) })
        }
        fn preview(&self, _: String, _: bool) -> BrowserFuture<Value> {
            Box::pin(async { Ok(Value::Null) })
        }
        fn respond(&self, _: String, _: bool, _: Vec<String>) -> BrowserFuture<Value> {
            Box::pin(async { Ok(json!({})) })
        }
    }

    #[tokio::test]
    async fn the_stream_owns_computer_use_while_it_listens() {
        let computer = NativeComputer {
            events: broadcast::channel(4).0,
        };
        assert!(!computer.connected());
        let mut tick = tokio::time::interval(Duration::from_millis(5));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let watch = Watch {
            host: Some(Arc::new(Fixed(json!([{ "requestId": "a" }])))),
            events: computer.events.subscribe(),
            tick,
            last: None,
        };
        assert!(computer.connected());
        let (_, watch) = next_frame(watch).await.unwrap();
        assert_eq!(watch.last.as_deref(), Some(r#"[{"requestId":"a"}]"#));
        // The same list is not sent again; a live event is.
        computer
            .events
            .send(("computer-activity".into(), json!({ "tool": "x" })))
            .unwrap();
        let (_, watch) = next_frame(watch).await.unwrap();
        drop(watch);
        assert!(!computer.connected());
    }
}
