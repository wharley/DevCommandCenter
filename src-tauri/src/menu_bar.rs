//! Small, read-only companion surface. No provider runtime or workbench is mounted.
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, System};
use tauri::{AppHandle, Emitter, Manager, Webview};

pub const LABEL: &str = "menu-bar";
#[cfg(target_os = "macos")]
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

pub struct MenuBarState {
    metrics: Mutex<MetricsSampler>,
}
struct MetricsSampler {
    system: System,
    last: Option<Instant>,
    value: Option<Metrics>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    cpu_percent: Option<f64>,
    memory_bytes: u64,
    process_count: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    workspace_id: String,
    session_id: String,
    title: String,
    project: String,
    status: String,
    updated_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    tasks: Vec<Task>,
    metrics: Option<Metrics>,
}

fn trusted(window: &Webview) -> Result<(), String> {
    if window.label() == LABEL {
        Ok(())
    } else {
        Err("unavailable".into())
    }
}

fn priority(status: &str) -> u8 {
    match status {
        "permission" | "input" => 0,
        "running" => 1,
        _ => 2,
    }
}

// Read a consistent snapshot without loading prompts, deltas or full histories.
// An active session alone is NOT evidence that a turn is running.
fn read_tasks(conn: &mut Connection) -> Result<Vec<Task>, rusqlite::Error> {
    let tx = conn.transaction()?;
    let mut tasks = HashMap::<String, Task>::new();
    let mut sessions = tx.prepare(
        "SELECT s.id, w.id, COALESCE(NULLIF(w.name,''), t.title),
                COALESCE(r.display_name, r.name, w.root_path)
         FROM dcc_sessions s JOIN dcc_threads t ON t.session_id = s.id
         JOIN dcc_workspaces w ON w.id = s.workspace_id
         LEFT JOIN dcc_repositories r ON r.project_id = w.project_id
         WHERE t.archived_at IS NULL AND w.state NOT IN ('archived','completed')",
    )?;
    let candidates = sessions
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut latest = tx.prepare(
        "SELECT sequence, json_extract(kind_json, '$.type'), occurred_at,
                json_extract(kind_json, '$.turnId')
         FROM dcc_session_events WHERE session_id = ?1
         AND json_extract(kind_json, '$.type') IN
           ('turn_started','session_completed','session_aborted')
         ORDER BY sequence DESC LIMIT 1",
    )?;
    let mut terminal = tx.prepare(
        "SELECT json_extract(kind_json, '$.type'), occurred_at
         FROM dcc_session_events WHERE session_id = ?1 AND sequence > ?2
         AND json_extract(kind_json, '$.turnId') = ?3
         AND json_extract(kind_json, '$.type') IN ('turn_completed','turn_aborted')
         ORDER BY sequence DESC LIMIT 1",
    )?;
    let mut pending = tx.prepare(
        "SELECT json_extract(kind_json, '$.type'), json_extract(kind_json, '$.requestId')
         FROM dcc_session_events WHERE session_id = ?1 AND sequence > ?2
         AND json_extract(kind_json, '$.turnId') = ?3
         AND json_extract(kind_json, '$.type') IN
           ('turn_permission_requested','turn_permission_resolved','turn_user_input_requested','turn_user_input_resolved')
         ORDER BY sequence"
    )?;
    for (session_id, workspace_id, title, project) in candidates {
        let mut rows = latest.query([&session_id])?;
        let Some(row) = rows.next()? else { continue };
        let sequence: i64 = row.get(0)?;
        let kind: String = row.get(1)?;
        let mut updated_at: String = row.get(2)?;
        let turn_id: Option<String> = row.get(3)?;
        if kind != "turn_started" {
            continue;
        }
        let mut status = "running";
        let mut terminal_rows = terminal.query(rusqlite::params![session_id, sequence, turn_id])?;
        if let Some(end) = terminal_rows.next()? {
            let kind: String = end.get(0)?;
            status = if kind == "turn_completed" {
                "completed"
            } else {
                "aborted"
            };
            updated_at = end.get(1)?;
        }
        if status == "running" {
            let mut requests = HashMap::new();
            let controls = pending
                .query_map(rusqlite::params![session_id, sequence, turn_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
            for control in controls {
                let (kind, id) = control?;
                let category = if kind.contains("permission") {
                    "permission"
                } else {
                    "input"
                };
                let key = (category, id);
                if kind.ends_with("_requested") {
                    requests.insert(key, category);
                } else {
                    requests.remove(&key);
                }
            }
            if requests.values().any(|value| *value == "permission") {
                status = "permission";
            } else if !requests.is_empty() {
                status = "input";
            }
        }
        let task = Task {
            workspace_id: workspace_id.clone(),
            session_id,
            title,
            project,
            status: status.into(),
            updated_at,
        };
        // A workspace is a task even when it has several conversations. Open the
        // conversation needing attention, then a running one, then the newest.
        let replace = tasks.get(&workspace_id).is_none_or(|old| {
            priority(&task.status) < priority(&old.status)
                || (priority(&task.status) == priority(&old.status)
                    && task.updated_at > old.updated_at)
        });
        if replace {
            tasks.insert(workspace_id, task);
        }
    }
    let mut tasks: Vec<_> = tasks.into_values().collect();
    tasks.sort_by(|a, b| {
        priority(&a.status)
            .cmp(&priority(&b.status))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.workspace_id.cmp(&b.workspace_id))
    });
    // Keep every active task and only the five latest inactive tasks.
    let mut recent = 0;
    tasks.retain(|task| {
        if priority(&task.status) < 2 {
            true
        } else {
            recent += 1;
            recent <= 5
        }
    });
    Ok(tasks)
}

fn descendants(roots: &[u32], parents: &[(u32, Option<u32>)]) -> HashSet<u32> {
    let mut included: HashSet<_> = roots.iter().copied().collect();
    loop {
        let before = included.len();
        for (pid, parent) in parents {
            if parent.is_some_and(|parent| included.contains(&parent)) {
                included.insert(*pid);
            }
        }
        if before == included.len() {
            return included;
        }
    }
}
impl MetricsSampler {
    fn sample(&mut self, roots: &[u32]) -> Metrics {
        if self
            .last
            .is_some_and(|last| last.elapsed() < Duration::from_secs(2))
        {
            if let Some(value) = &self.value {
                return value.clone();
            }
        }
        // Processes only; no disks, network, sensors or per-task polling.
        self.system
            .refresh_processes_specifics(ProcessRefreshKind::new().with_cpu().with_memory());
        let parents = self
            .system
            .processes()
            .iter()
            .map(|(pid, p)| (pid.as_u32(), p.parent().map(|id| id.as_u32())))
            .collect::<Vec<_>>();
        let included = descendants(roots, &parents);
        let processes: Vec<_> = included
            .iter()
            .filter_map(|id| self.system.process(Pid::from_u32(*id)))
            .collect();
        // A fresh/long-paused sampler needs two samples; never show a fake zero.
        let warm = self
            .last
            .is_some_and(|last| last.elapsed() < Duration::from_secs(10));
        let value = Metrics {
            cpu_percent: warm.then(|| processes.iter().map(|p| p.cpu_usage() as f64).sum()),
            memory_bytes: processes.iter().map(|p| p.memory()).sum(),
            process_count: processes.len(),
        };
        self.last = Some(Instant::now());
        self.value = Some(value.clone());
        value
    }
}

#[tauri::command]
pub async fn menu_bar_snapshot(window: Webview, app: AppHandle) -> Result<Snapshot, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        let mut conn =
            Connection::open_with_flags(state.db_path.as_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?;
        conn.busy_timeout(Duration::from_millis(300))
            .map_err(|e| e.to_string())?;
        let tasks = read_tasks(&mut conn).map_err(|e| e.to_string())?;
        let mut roots = vec![std::process::id()];
        if let Ok(endpoint) = state.daemon_endpoint.lock() {
            if let Some(endpoint) = endpoint.as_ref() {
                roots.push(endpoint.pid);
            }
        }
        let metrics = app
            .state::<MenuBarState>()
            .metrics
            .lock()
            .ok()
            .map(|mut sampler| sampler.sample(&roots));
        Ok(Snapshot { tasks, metrics })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn menu_bar_visible(window: Webview, app: AppHandle) -> Result<bool, String> {
    trusted(&window)?;
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let visible = unsafe { native::dcc_menu_bar_visible() };
        #[cfg(not(target_os = "macos"))]
        let visible = false;
        let _ = send.send(visible);
    })
    .map_err(|e| e.to_string())?;
    receive.await.map_err(|e| e.to_string())
}

fn hide(app: &AppHandle) -> Result<(), String> {
    app.run_on_main_thread(|| {
        #[cfg(target_os = "macos")]
        unsafe {
            native::dcc_menu_bar_hide();
        }
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn menu_bar_hide(window: Webview, app: AppHandle) -> Result<(), String> {
    trusted(&window)?;
    hide(&app)
}
#[tauri::command]
pub fn menu_bar_compose(window: Webview, app: AppHandle) -> Result<(), String> {
    trusted(&window)?;
    hide(&app)?;
    crate::quick_composer::show(&app)
}
pub fn show_main(app: &AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or("Main window unavailable")?;
    main.show().map_err(|e| e.to_string())?;
    main.unminimize().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn menu_bar_open_main(
    window: Webview,
    app: AppHandle,
    workspace_id: Option<String>,
    session_id: Option<String>,
) -> Result<(), String> {
    trusted(&window)?;
    hide(&app)?;
    show_main(&app)?;
    app.emit_to(
        "main",
        "menu-bar-open-task",
        serde_json::json!({
            "workspaceId": workspace_id, "sessionId": session_id
        }),
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn menu_bar_quit(window: Webview, app: AppHandle) -> Result<(), String> {
    trusted(&window)?;
    app.exit(0);
    Ok(())
}

pub fn setup(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let panel = tauri::WebviewWindowBuilder::new(
            app,
            LABEL,
            tauri::WebviewUrl::App("index.html?menu-bar=1".into()),
        )
        .title("DCC")
        .inner_size(368., 480.)
        .visible(false)
        .focused(false)
        .resizable(false)
        .skip_taskbar(true)
        .on_navigation(crate::quick_composer::local_navigation)
        .build()
        .map_err(|e| e.to_string())?;
        let _ = APP.set(app.clone());
        if !unsafe {
            native::dcc_menu_bar_create(
                panel.ns_window().map_err(|e| e.to_string())?,
                visibility_changed,
            )
        } {
            let _ = panel.destroy();
            return Err("Could not create menu bar item".into());
        }
        app.manage(MenuBarState {
            metrics: Mutex::new(MetricsSampler {
                system: System::new(),
                last: None,
                value: None,
            }),
        });
    }
    let _ = app;
    Ok(())
}
#[cfg(target_os = "macos")]
extern "C" fn visibility_changed(visible: bool) {
    if let Some(app) = APP.get() {
        let _ = app.emit_to(LABEL, "menu-bar-visibility", visible);
    }
}
pub fn shutdown(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    unsafe {
        native::dcc_menu_bar_destroy();
    }
    if let Some(panel) = app.get_webview_window(LABEL) {
        let _ = panel.destroy();
    }
}
#[cfg(target_os = "macos")]
mod native {
    extern "C" {
        pub fn dcc_menu_bar_create(
            window: *mut std::ffi::c_void,
            callback: extern "C" fn(bool),
        ) -> bool;
        pub fn dcc_menu_bar_visible() -> bool;
        pub fn dcc_menu_bar_hide();
        pub fn dcc_menu_bar_destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE dcc_sessions (id TEXT, workspace_id TEXT);
            CREATE TABLE dcc_threads (session_id TEXT, title TEXT, archived_at TEXT);
            CREATE TABLE dcc_workspaces (id TEXT, project_id TEXT, name TEXT, root_path TEXT, state TEXT);
            CREATE TABLE dcc_repositories (project_id TEXT, display_name TEXT, name TEXT);
            CREATE TABLE dcc_session_events (session_id TEXT, sequence INTEGER, kind_json TEXT, occurred_at TEXT);
            CREATE INDEX events ON dcc_session_events(session_id, sequence);
            INSERT INTO dcc_repositories VALUES ('project', 'My project', 'repo');
            INSERT INTO dcc_workspaces VALUES ('w', 'project', 'My task', '/repo', 'ready');
            INSERT INTO dcc_sessions VALUES ('s', 'w');
            INSERT INTO dcc_threads VALUES ('s', 'Conversation', NULL);").unwrap();
        conn
    }
    fn event(conn: &Connection, session: &str, seq: i64, kind: &str, turn: &str, request: &str) {
        conn.execute(
            "INSERT INTO dcc_session_events VALUES (?1,?2,?3,?4)",
            rusqlite::params![
                session,
                seq,
                serde_json::json!({"type":kind,"turnId":turn,"requestId":request}).to_string(),
                format!("2026-09-24T12:{seq:02}:00Z")
            ],
        )
        .unwrap();
    }
    #[test]
    fn idle_sessions_are_not_running_and_terminal_turns_clear_pending_requests() {
        let mut conn = database();
        assert!(read_tasks(&mut conn).unwrap().is_empty());
        event(&conn, "s", 1, "turn_started", "t", "");
        event(&conn, "s", 2, "turn_user_input_requested", "t", "q");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "input");
        event(&conn, "s", 3, "turn_completed", "t", "");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "completed");
        event(&conn, "s", 4, "turn_started", "next", "");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "running");
        event(&conn, "s", 5, "session_aborted", "", "");
        assert!(read_tasks(&mut conn).unwrap().is_empty());
    }
    #[test]
    fn resolves_only_matching_requests_and_ignores_other_turns() {
        let mut conn = database();
        event(&conn, "s", 1, "turn_started", "t", "");
        event(&conn, "s", 2, "turn_permission_requested", "t", "a");
        event(&conn, "s", 3, "turn_permission_requested", "t", "b");
        event(&conn, "s", 4, "turn_permission_resolved", "t", "a");
        event(&conn, "s", 5, "turn_user_input_requested", "old", "q");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "permission");
        event(&conn, "s", 6, "turn_permission_resolved", "t", "b");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "running");
    }
    #[test]
    fn groups_conversations_prioritizes_attention_and_excludes_archived_tasks() {
        let mut conn = database();
        conn.execute_batch(
            "INSERT INTO dcc_sessions VALUES ('s2','w');
            INSERT INTO dcc_threads VALUES ('s2','Other conversation',NULL);",
        )
        .unwrap();
        event(&conn, "s", 1, "turn_started", "t", "");
        event(&conn, "s", 2, "turn_permission_requested", "t", "a");
        event(&conn, "s2", 3, "turn_started", "t2", "");
        let tasks = read_tasks(&mut conn).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].session_id, "s");
        assert_eq!(tasks[0].project, "My project");
        conn.execute(
            "UPDATE dcc_threads SET archived_at='now' WHERE session_id='s'",
            [],
        )
        .unwrap();
        assert_eq!(read_tasks(&mut conn).unwrap()[0].session_id, "s2");
        conn.execute("UPDATE dcc_workspaces SET state='archived'", [])
            .unwrap();
        assert!(read_tasks(&mut conn).unwrap().is_empty());
    }
    #[test]
    fn a_late_terminal_event_from_an_old_turn_does_not_hide_current_work() {
        let mut conn = database();
        event(&conn, "s", 1, "turn_started", "old", "");
        event(&conn, "s", 2, "turn_started", "current", "");
        event(&conn, "s", 3, "turn_aborted", "old", "");
        assert_eq!(read_tasks(&mut conn).unwrap()[0].status, "running");
    }
    #[test]
    fn process_tree_deduplicates_roots_and_excludes_unrelated_processes() {
        let included = descendants(
            &[10, 20],
            &[
                (40, Some(30)),
                (30, Some(20)),
                (20, Some(10)),
                (99, Some(1)),
            ],
        );
        assert_eq!(included, HashSet::from([10, 20, 30, 40]));
    }
}
