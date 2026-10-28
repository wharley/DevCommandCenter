//! The agents' `dcc_browser_open` requests, for the native app's Browser.
//! While the native app listens on `GET /api/v1/browser/bridge/opens/stream`
//! it owns them: it gets the requests waiting for the person
//! (`open-requests`, again whenever they change) and the ones a phone or
//! the desktop's panel allowed (`open-approved`), answers with `allow` and
//! `deny` (first answer wins, as everywhere), and confirms the open with
//! its Browser's lifecycle token (`opened`). The desktop's own dialog and
//! panel stay quiet meanwhile. Local clients only, like the rest of the
//! bridge.
use super::browser_bridge::DesktopBrowser;
use super::*;
use tokio::sync::broadcast;

/// How often the waiting requests are read for the stream.
const POLL: Duration = Duration::from_millis(500);

pub struct NativeBrowserOpens {
    approved: broadcast::Sender<Value>,
}

pub fn native_browser_opens() -> &'static NativeBrowserOpens {
    static OPENS: OnceLock<NativeBrowserOpens> = OnceLock::new();
    OPENS.get_or_init(|| NativeBrowserOpens {
        approved: broadcast::channel(16).0,
    })
}

impl NativeBrowserOpens {
    /// Whether the native app answers the Browser's open requests now.
    pub fn connected(&self) -> bool {
        self.approved.receiver_count() > 0
    }

    /// A request allowed elsewhere, for the native app to open; `false`
    /// when it is not listening.
    pub fn approved(&self, request: Value) -> bool {
        self.approved.send(request).is_ok()
    }
}

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/browser/bridge/opens/stream", get(stream))
        .route("/api/v1/browser/bridge/opens/:id/allow", post(allow))
        .route("/api/v1/browser/bridge/opens/:id/deny", post(deny))
        .route("/api/v1/browser/bridge/opens/:id/opened", post(opened))
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
struct ScopeInput {
    workspace_id: String,
    session_id: String,
    #[serde(default)]
    lifecycle_token: Option<u64>,
}

async fn allow(
    State(backend): State<Arc<HttpBackend>>,
    Path(id): Path<String>,
    Json(input): Json<ScopeInput>,
) -> Result<Json<Value>, HttpApiError> {
    let request = host(&backend)?
        .open_allow(id, input.workspace_id, input.session_id)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(request))
}

async fn deny(
    State(backend): State<Arc<HttpBackend>>,
    Path(id): Path<String>,
    Json(input): Json<ScopeInput>,
) -> Result<Json<Value>, HttpApiError> {
    host(&backend)?
        .open_deny(id, input.workspace_id, input.session_id)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

async fn opened(
    State(backend): State<Arc<HttpBackend>>,
    Path(id): Path<String>,
    Json(input): Json<ScopeInput>,
) -> Result<Json<Value>, HttpApiError> {
    let token = input
        .lifecycle_token
        .ok_or_else(|| HttpApiError::bad_request("browser open lifecycle is required"))?;
    let result = host(&backend)?
        .open_ack(id, input.workspace_id, input.session_id, token)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(result))
}

/// What the stream keeps between frames.
struct Watch {
    host: Option<Arc<dyn DesktopBrowser>>,
    approved: broadcast::Receiver<Value>,
    tick: tokio::time::Interval,
    last: Option<String>,
}

async fn next_frame(mut watch: Watch) -> Option<(Result<Event, std::convert::Infallible>, Watch)> {
    loop {
        tokio::select! {
            approved = watch.approved.recv() => match approved {
                Ok(request) => {
                    let event = Event::default().event("open-approved").data(request.to_string());
                    return Some((Ok(event), watch));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return None,
            },
            _ = watch.tick.tick() => {
                let requests = watch
                    .host
                    .as_ref()
                    .map(|host| host.open_requests())
                    .unwrap_or_else(|| json!([]))
                    .to_string();
                if watch.last.as_deref() != Some(requests.as_str()) {
                    watch.last = Some(requests.clone());
                    let event = Event::default().event("open-requests").data(requests);
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
        host: backend.desktop_browser.get().cloned(),
        approved: native_browser_opens().approved.subscribe(),
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

    #[tokio::test]
    async fn the_stream_owns_the_opens_while_it_listens() {
        let opens = NativeBrowserOpens {
            approved: broadcast::channel(4).0,
        };
        assert!(!opens.connected());
        assert!(
            !opens.approved(json!({ "requestId": "r-1" })),
            "nobody to open it"
        );
        let mut tick = tokio::time::interval(Duration::from_millis(5));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let watch = Watch {
            host: None,
            approved: opens.approved.subscribe(),
            tick,
            last: None,
        };
        assert!(opens.connected());
        // The waiting requests first; the same list is not sent twice.
        let (_, watch) = next_frame(watch).await.unwrap();
        assert_eq!(watch.last.as_deref(), Some("[]"));
        assert!(opens.approved(json!({ "requestId": "r-2" })));
        let (_, watch) = next_frame(watch).await.unwrap();
        assert_eq!(watch.last.as_deref(), Some("[]"));
        drop(watch);
        assert!(!opens.connected());
    }
}
