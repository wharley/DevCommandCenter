//! The desktop's own terminals for the native app: the same PTYs the
//! WebView's dock attaches to (`terminal_get_or_create` and friends in
//! `main.rs`), reached through [`DesktopTerminals`]. Phones never get here:
//! the relay refuses every `/api/v1/terminals/` route.
use super::*;
use tokio::sync::broadcast;

/// What the desktop's PTY runtime offers the HTTP API.
pub trait DesktopTerminals: Send + Sync {
    /// `terminal_get_or_create`: the owner's shell, or a new one, with what
    /// it printed so far (`{ ptyId, existing, session, chunks, truncated }`).
    fn get_or_create(&self, owner_key: &str, options: Value) -> Result<Value, String>;
    /// Whether the PTY was there to take the input.
    fn write(&self, pty_id: &str, data: String) -> Result<bool, String>;
    fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<bool, String>;
    fn kill(&self, pty_id: &str) -> Result<bool, String>;
    /// `terminal_list_activity`.
    fn activity(&self) -> Result<Vec<Value>, String>;
}

/// What a PTY printed, or its end, as the WebView gets them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DesktopTerminalEvent {
    #[serde(rename_all = "camelCase")]
    Output { pty_id: String, data: String },
    #[serde(rename_all = "camelCase")]
    Exit { pty_id: String, code: Option<i64> },
}

impl DesktopTerminalEvent {
    fn name(&self) -> &'static str {
        match self {
            Self::Output { .. } => "terminal-output",
            Self::Exit { .. } => "terminal-exit",
        }
    }
}

/// About 16 seconds of output at the runtime's 16ms batches.
const EVENT_CAPACITY: usize = 1024;

fn events() -> &'static broadcast::Sender<DesktopTerminalEvent> {
    static EVENTS: OnceLock<broadcast::Sender<DesktopTerminalEvent>> = OnceLock::new();
    EVENTS.get_or_init(|| broadcast::channel(EVENT_CAPACITY).0)
}

/// Called by the runtime next to each `terminal-output` / `terminal-exit`.
pub fn publish_desktop_terminal_event(event: DesktopTerminalEvent) {
    let _ = events().send(event);
}

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/terminals/events/stream", get(stream))
        .route("/api/v1/terminals/activity", get(activity))
        .route("/api/v1/terminals/by-owner/:owner_key", post(by_owner))
        .route("/api/v1/terminals/:pty_id/write", post(write))
        .route("/api/v1/terminals/:pty_id/resize", post(resize))
        .route("/api/v1/terminals/:pty_id/kill", post(kill))
}

fn host(backend: &HttpBackend) -> Result<Arc<dyn DesktopTerminals>, HttpApiError> {
    backend
        .desktop_terminals
        .get()
        .cloned()
        .ok_or_else(|| HttpApiError::internal("the desktop's terminals are not attached"))
}

fn ok(result: Result<bool, String>) -> Result<Json<Value>, HttpApiError> {
    let ok = result.map_err(HttpApiError::internal)?;
    Ok(Json(json!({ "ok": ok })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ByOwnerRequest {
    cwd: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

/// The app's own commands with no task to hold them ("Conectar GitHub",
/// an agent CLI's update): the only shells that may open in the person's
/// home folder.
const HOME_OWNERS: [&str; 2] = ["forge-connect:", "cli-update:"];

/// Whether `cwd` is the person's home folder.
fn is_home(cwd: &str) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let canonical = |path: &std::path::Path| std::fs::canonicalize(path).ok();
    canonical(std::path::Path::new(cwd))
        .is_some_and(|cwd| Some(cwd) == canonical(std::path::Path::new(&home)))
}

/// The shell opens in a task's checkout (its worktree or its repository),
/// or in the home folder for one of the app's own commands (`HOME_OWNERS`).
async fn by_owner(
    State(backend): State<Arc<HttpBackend>>,
    Path(owner_key): Path<String>,
    Json(input): Json<ByOwnerRequest>,
) -> Result<Json<Value>, HttpApiError> {
    let app_command = HOME_OWNERS
        .iter()
        .any(|prefix| owner_key.starts_with(prefix));
    if !(app_command && is_home(&input.cwd)) {
        git_changes::task_checkout(&backend, &input.cwd).await?;
    }
    let host = host(&backend)?;
    let options = json!({
        "cwd": input.cwd,
        "command": input.command,
        "args": input.args,
        "cols": input.cols.unwrap_or(120),
        "rows": input.rows.unwrap_or(32),
        "ptyOwnerKey": owner_key,
    });
    let value = tokio::task::spawn_blocking(move || host.get_or_create(&owner_key, options))
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(value))
}

async fn write(
    State(backend): State<Arc<HttpBackend>>,
    Path(pty_id): Path<String>,
    Json(input): Json<TerminalWriteRequest>,
) -> Result<Json<Value>, HttpApiError> {
    ok(host(&backend)?.write(&pty_id, input.data))
}

async fn resize(
    State(backend): State<Arc<HttpBackend>>,
    Path(pty_id): Path<String>,
    Json(input): Json<TerminalResizeRequest>,
) -> Result<Json<Value>, HttpApiError> {
    ok(host(&backend)?.resize(&pty_id, input.cols, input.rows))
}

async fn kill(
    State(backend): State<Arc<HttpBackend>>,
    Path(pty_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    ok(host(&backend)?.kill(&pty_id))
}

async fn activity(State(backend): State<Arc<HttpBackend>>) -> Result<Json<Value>, HttpApiError> {
    let host = host(&backend)?;
    let items = tokio::task::spawn_blocking(move || host.activity())
        .await
        .map_err(|error| HttpApiError::internal(error.to_string()))?
        .map_err(HttpApiError::internal)?;
    Ok(Json(json!({ "items": items })))
}

/// Every PTY's output and end. A client that fell behind gets
/// `terminal-lagged` and reads its terminals again (`by-owner`).
async fn stream() -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>>
{
    let stream = BroadcastStream::new(events().subscribe()).filter_map(|message| match message {
        Ok(event) => serde_json::to_string(&event)
            .ok()
            .map(|payload| Ok(Event::default().event(event.name()).data(payload))),
        Err(_) => Some(Ok(Event::default().event("terminal-lagged").data("{}"))),
    });
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
    fn events_read_like_the_webview_payloads() {
        let output = DesktopTerminalEvent::Output {
            pty_id: "pty-1".into(),
            data: "ok\r\n".into(),
        };
        assert_eq!(
            serde_json::to_value(&output).unwrap(),
            json!({ "kind": "output", "ptyId": "pty-1", "data": "ok\r\n" })
        );
        assert_eq!(output.name(), "terminal-output");
        let exit = DesktopTerminalEvent::Exit {
            pty_id: "pty-1".into(),
            code: Some(0),
        };
        assert_eq!(
            serde_json::to_value(&exit).unwrap(),
            json!({ "kind": "exit", "ptyId": "pty-1", "code": 0 })
        );
    }
}
