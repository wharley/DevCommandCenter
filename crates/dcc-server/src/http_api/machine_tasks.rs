//! Tasks that run on a machine, for the app on this computer: creating one
//! there and keeping its mirror here (`crate::machine_mirror`).
//!
//! - `POST /api/v1/machines/:id/tasks` asks the machine for a task in one of
//!   its projects (its own `POST /api/v1/workspaces/create`, the same
//!   creation "Nova tarefa" runs locally), copies its rows here and starts
//!   following it. The task has no session yet: the first prompt starts one
//!   on the machine, and the next follow finds it.
//! - `POST /api/v1/machines/tasks/follow` follows every mirrored task again
//!   (the app asks when it opens and after a first prompt: a follower ends
//!   when its machine leaves, or while the task has no session).
//!
//! Following a task is push, not polling: the machine's event stream on the
//! channel says something happened, and only then are its new events read
//! (`…/events?after=<cursor>`), with the sequence the machine gave them.
use std::time::Duration;

use dcc_core::domain::session::{SessionEventKind, SessionEventRecord, WorkspaceSessionSummary};
use dcc_core::domain::workspace::Workspace;
use dcc_core::ports::{SessionRepo, ThreadRepo, WorkspaceRepo};

use super::*;
use crate::machine_link::StreamItem;
use crate::machine_mirror::{self, machine_path, MirroredTask};

/// Events arriving together are read in one request.
const SETTLE: Duration = Duration::from_millis(200);
const EVENTS_PAGE: usize = 500;

pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/machines/tasks/follow", post(follow_all))
        .route("/api/v1/machines/:id/tasks", post(create))
}

fn operator(device: &Option<axum::Extension<PairedDeviceIdentity>>) -> Result<(), HttpApiError> {
    if device.is_some() {
        return Err(HttpApiError::Forbidden(
            "paired devices cannot reach other machines".into(),
        ));
    }
    Ok(())
}

/// An id as it goes in a path: the machine's ids are UUIDs, and anything
/// else is refused instead of escaped.
pub(super) fn id(value: &str) -> Result<&str, String> {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    plain
        .then_some(value)
        .ok_or_else(|| "unexpected id".to_string())
}

/// One JSON call of the machine's API; anything but 2xx is its error.
async fn machine_json(
    backend: &HttpBackend,
    machine_id: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let mut headers = std::collections::BTreeMap::new();
    headers.insert("accept".to_string(), "application/json".to_string());
    let payload = match body {
        Some(body) => {
            headers.insert("content-type".to_string(), "application/json".to_string());
            body.to_string().into_bytes()
        }
        None => Vec::new(),
    };
    let answer = backend
        .machines()
        .request(machine_id, method, path, headers, payload)
        .await
        .map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_slice(&answer.body).unwrap_or(Value::Null);
    if (200..300).contains(&answer.status) {
        return Ok(value);
    }
    Err(value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("the machine answered {}", answer.status)))
}

async fn with_mirror<T: Send + 'static>(
    backend: &HttpBackend,
    work: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let path = backend.workspace.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let mut conn = machine_mirror::open(&path).map_err(|error| error.to_string())?;
        work(&mut conn)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Copies the task's workspace, session and thread as the machine has them
/// now (its name, state and title change as it runs), with its paths marked
/// as the machine's. Answers the task's session, empty while it has none.
async fn copy_rows(backend: &HttpBackend, task: &MirroredTask) -> Result<String, String> {
    let machine = task.machine_id.as_str();
    let catalog = machine_json(backend, machine, "GET", "/api/v1/mobile/catalog", None).await?;
    let workspaces: Vec<Workspace> =
        serde_json::from_value(catalog.get("workspaces").cloned().unwrap_or(Value::Null))
            .map_err(|error| error.to_string())?;
    let mut workspace = workspaces
        .into_iter()
        .find(|workspace| workspace.id.0 == task.workspace_id)
        .ok_or_else(|| "the machine no longer has this task".to_string())?;
    workspace.root_path = machine_path(machine, &workspace.root_path);
    workspace.worktree_path = workspace
        .worktree_path
        .map(|path| machine_path(machine, &path));
    WorkspaceRepo::save_workspace(&backend.sessions, &workspace)
        .await
        .map_err(|error| error.to_string())?;

    let path = format!("/api/v1/sessions?workspaceId={}", id(&task.workspace_id)?);
    let sessions: Vec<WorkspaceSessionSummary> =
        serde_json::from_value(machine_json(backend, machine, "GET", &path, None).await?)
            .map_err(|error| error.to_string())?;
    // The task's session: the one already known, else the one its first
    // prompt started on the machine (kept from now on).
    let Some(summary) = sessions
        .into_iter()
        .find(|summary| task.session_id.is_empty() || summary.session.id.0 == task.session_id)
    else {
        return Ok(task.session_id.clone());
    };
    let session_id = summary.session.id.0.clone();
    let mut session = summary.session;
    session.working_directory_override = session
        .working_directory_override
        .map(|path| machine_path(machine, &path));
    SessionRepo::save_session(&backend.sessions, &session)
        .await
        .map_err(|error| error.to_string())?;
    ThreadRepo::save_thread(&backend.sessions, &summary.thread)
        .await
        .map_err(|error| error.to_string())?;
    if task.session_id.is_empty() {
        let (workspace, machine, session) = (
            task.workspace_id.clone(),
            task.machine_id.clone(),
            session_id.clone(),
        );
        with_mirror(backend, move |conn| {
            machine_mirror::record(conn, &workspace, &machine, &session)
                .map_err(|error| error.to_string())
        })
        .await?;
    }
    Ok(session_id)
}

/// What one read of the machine's events brought.
#[derive(Debug, Default)]
pub(super) struct EventsPage {
    pub events: Vec<SessionEventRecord>,
    pub has_more: bool,
    /// The machine merged events the cursor had only part of.
    pub reset: bool,
}

pub(super) fn events_page(value: Value) -> Result<EventsPage, String> {
    if value.get("reset").and_then(Value::as_bool) == Some(true) {
        return Ok(EventsPage {
            reset: true,
            ..EventsPage::default()
        });
    }
    let events = serde_json::from_value(value.get("events").cloned().unwrap_or(Value::Null))
        .map_err(|error| error.to_string())?;
    Ok(EventsPage {
        events,
        has_more: value.get("hasMore").and_then(Value::as_bool) == Some(true),
        reset: false,
    })
}

/// Reads the session's events past the cursor into the mirror. True when a
/// turn ended among them (the task's rows may have changed with it).
async fn copy_events(backend: &HttpBackend, workspace_id: &str) -> Result<bool, String> {
    let mut turn_ended = false;
    loop {
        let wanted = workspace_id.to_string();
        let task = with_mirror(backend, move |conn| {
            machine_mirror::find(conn, &wanted).map_err(|error| error.to_string())
        })
        .await?
        .ok_or_else(|| "not a mirrored task".to_string())?;
        let path = format!(
            "/api/v1/sessions/{}/events?after={}&limit={EVENTS_PAGE}",
            id(&task.session_id)?,
            task.cursor
        );
        let page = events_page(machine_json(backend, &task.machine_id, "GET", &path, None).await?)?;
        if page.reset {
            let (workspace, session) = (task.workspace_id.clone(), task.session_id.clone());
            with_mirror(backend, move |conn| {
                machine_mirror::reset(conn, &workspace, &session)
            })
            .await?;
            continue;
        }
        turn_ended |= page.events.iter().any(|event| {
            matches!(
                event.kind,
                SessionEventKind::TurnCompleted { .. } | SessionEventKind::TurnAborted { .. }
            )
        });
        let has_more = page.has_more && !page.events.is_empty();
        let workspace = task.workspace_id.clone();
        with_mirror(backend, move |conn| {
            machine_mirror::apply_events(conn, &workspace, &page.events)
        })
        .await?;
        if !has_more {
            return Ok(turn_ended);
        }
    }
}

/// Follows a mirrored task until its machine leaves: copies what is there,
/// then waits on the machine's stream and copies again each time it says
/// something happened.
async fn follow(backend: Arc<HttpBackend>, mut task: MirroredTask) -> Result<(), String> {
    task.session_id = copy_rows(&backend, &task).await?;
    if task.session_id.is_empty() {
        // No prompt was sent yet: nothing to follow until one is.
        return Ok(());
    }
    copy_events(&backend, &task.workspace_id).await?;
    let stream_path = format!("/api/v1/events/stream?sessionId={}", id(&task.session_id)?);
    let mut stream = backend
        .machines()
        .subscribe(&task.machine_id, &stream_path)
        .await
        .map_err(|error| error.to_string())?;
    // Anything that came between the copy and the subscription.
    copy_events(&backend, &task.workspace_id).await?;
    while let Some(StreamItem::Event(_)) = stream.recv().await {
        tokio::time::sleep(SETTLE).await;
        while stream.try_recv().is_ok() {}
        if copy_events(&backend, &task.workspace_id).await? {
            copy_rows(&backend, &task).await?;
        }
    }
    Ok(())
}

/// Starts following in the background; a task already followed is left to
/// its follower.
fn spawn_follow(backend: &Arc<HttpBackend>, task: MirroredTask) -> bool {
    let machines = backend.machines();
    if !machines.start_following(&task.workspace_id) {
        return false;
    }
    let backend = backend.clone();
    tokio::spawn(async move {
        let workspace_id = task.workspace_id.clone();
        if let Err(error) = follow(backend.clone(), task).await {
            eprintln!("[DCC machines] stopped following {workspace_id}: {error}");
        }
        backend.machines().stop_following(&workspace_id);
    });
    true
}

async fn follow_all(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    let tasks = with_mirror(&backend, |conn| {
        machine_mirror::list(conn).map_err(|error| error.to_string())
    })
    .await
    .map_err(HttpApiError::internal)?;
    let started = tasks
        .into_iter()
        .filter(|task| spawn_follow(&backend, task.clone()))
        .count();
    Ok(Json(json!({ "started": started })))
}

/// The new task's id in the machine's answer to its creation.
pub(super) fn created_workspace(answer: &Value) -> Option<String> {
    answer
        .get("workspaceId")
        .and_then(Value::as_str)
        .filter(|workspace| id(workspace).is_ok())
        .map(str::to_string)
}

async fn create(
    State(backend): State<Arc<HttpBackend>>,
    device: Option<axum::Extension<PairedDeviceIdentity>>,
    Path(machine_id): Path<String>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, HttpApiError> {
    operator(&device)?;
    // The machine's own creation, with the fields "Nova tarefa" sends.
    let answer = machine_json(
        &backend,
        &machine_id,
        "POST",
        "/api/v1/workspaces/create",
        Some(&input),
    )
    .await
    .map_err(HttpApiError::BadGateway)?;
    let workspace_id = created_workspace(&answer)
        .ok_or_else(|| HttpApiError::BadGateway("the machine created no task".into()))?;
    let (workspace, machine) = (workspace_id.clone(), machine_id.clone());
    with_mirror(&backend, move |conn| {
        machine_mirror::record(conn, &workspace, &machine, "").map_err(|error| error.to_string())
    })
    .await
    .map_err(HttpApiError::internal)?;
    let task = MirroredTask {
        workspace_id,
        machine_id,
        session_id: String::new(),
        cursor: 0,
    };
    // The rows first, so the task is in the list when this answers.
    copy_rows(&backend, &task)
        .await
        .map_err(HttpApiError::BadGateway)?;
    Ok(Json(answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_of_events_says_when_the_machine_rewrote_the_session() {
        let page =
            events_page(json!({ "events": [], "lastSeq": 0, "hasMore": false, "reset": true }))
                .unwrap();
        assert!(page.reset);
        let page = events_page(json!({
            "events": [{ "eventId": "e-1", "sessionId": "s-1", "sequence": 4,
                "occurredAt": "2026-11-13T10:00:00Z",
                "kind": { "type": "turn_completed", "turnId": "t-1" } }],
            "lastSeq": 9, "hasMore": true,
        }))
        .unwrap();
        assert_eq!(
            (page.events.len(), page.has_more, page.reset),
            (1, true, false)
        );
        assert_eq!(page.events[0].sequence, 4);
    }

    #[test]
    fn an_id_that_is_not_plain_never_reaches_a_path() {
        assert_eq!(id("0b7c-ab_1"), Ok("0b7c-ab_1"));
        assert!(id("a/../b").is_err());
        assert!(id("a?x=1").is_err());
        assert!(id("").is_err());
    }

    #[test]
    fn a_created_task_is_read_from_the_machines_answer() {
        let answer = json!({ "workspaceId": "w-1", "setupReport": null, "setupHints": [] });
        assert_eq!(created_workspace(&answer).as_deref(), Some("w-1"));
        assert_eq!(created_workspace(&json!({ "workspaceId": "../x" })), None);
        assert_eq!(created_workspace(&json!({})), None);
    }
}
