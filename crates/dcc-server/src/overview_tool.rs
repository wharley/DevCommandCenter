//! `dcc_overview`: in a resident agent's own conversation, the person's
//! projects and tasks across DCC — what needs them (the same "Precisa de
//! você" the phone reads, failing checks and conflicts included), what runs,
//! what finished recently — with times in the person's own time zone. Tasks
//! come from the database like the menu bar's snapshot; the blockers reuse
//! the attention cache, so a question costs no more than the phone's poll.
//! Other sessions get an error.

use std::collections::HashMap;
use std::time::Duration;

use dcc_core::domain::agent_conversation::OVERVIEW_TOOL;
use dcc_core::domain::attention::{AttentionItem, AttentionReason};
use dcc_core::domain::session::SessionId;
use dcc_tauri::commands::workspace_attention::workspace_attention_with_state;
use dcc_tauri::state::{SessionCommandState, WorkspaceCommandState};
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use crate::menu_bar_snapshot::{read_tasks_with, Task};

/// Finished tasks listed besides every active one.
const RECENT_TASKS: usize = 15;
/// Blockers this fresh are reused (they run git for every task).
const BLOCKERS_MAX_AGE: Duration = Duration::from_secs(60);

pub fn is_overview_tool(name: &str) -> bool {
    name == OVERVIEW_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": OVERVIEW_TOOL,
        "description": "Only in your own conversation as a DCC resident agent (your instructions carry a dcc_agent_conversation block): the person's projects and tasks across DCC. projects lists each project with its name, its repository (host/owner/name) and folder: that, not the name alone, is what says whose code a project is. needsYou is DCC's \"Precisa de você\" list, each with its reason: permission or input (the agent waits on them), checksFailing or prConflicts (with the PR number), conflicts, delegatedReview, usageLimit, woke, setup. running has a turn in progress; recent are the latest others, newest first. Every time is the person's local time with its UTC offset; quote it as local time. Read it before answering what needs the person, how the day went or where a task stands.",
        "inputSchema": {"type": "object", "additionalProperties": false, "properties": {}},
        "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    match arguments {
        None | Some(Value::Null) => true,
        Some(Value::Object(map)) => map.is_empty(),
        Some(_) => false,
    }
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

/// `at` in the machine's time zone, as written; unreadable times stay as is.
pub(crate) fn local_time(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        })
        .unwrap_or_else(|_| at.to_string())
}

fn task_view(task: &Task) -> Value {
    json!({
        "task": task.title,
        "project": task.project,
        "status": task.status,
        "updatedAt": local_time(&task.updated_at),
    })
}

/// Where a repository lives, as its remote says it without the scheme, a
/// user or a password: `github.com/acme/loja`. It is what tells one
/// project from another of the same name.
fn repository_of(remote_url: &str) -> Option<String> {
    let url = remote_url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    // The user (and a password) end at the last `@` before the path.
    let head = rest.split_once('/').map_or(rest, |(head, _)| head);
    let rest = head
        .rfind('@')
        .map_or(rest, |at| rest.get(at + 1..).unwrap_or(rest));
    let (host, path) = rest.split_once(['/', ':'])?;
    let path = path.trim_matches('/');
    (!host.is_empty() && !path.is_empty()).then(|| format!("{host}/{path}"))
}

/// The person's projects: each with its name and where its code lives.
/// `only`: the projects chosen for the agent (empty: every project).
fn projects(conn: &Connection, only: &[String]) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT project_id, MIN(COALESCE(NULLIF(display_name, ''), name)) AS label,
                MIN(remote_url), MIN(root_path)
         FROM dcc_repositories
         WHERE project_id NOT LIKE 'dcc-agent-%'
         GROUP BY project_id ORDER BY lower(label)",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|(id, ..)| dcc_core::domain::agent_projects::works_in(only, id))
        .map(|(_, name, remote, folder)| {
            let mut project = json!({ "name": name });
            if let Some(repository) = remote.as_deref().and_then(repository_of) {
                project["repository"] = json!(repository);
            }
            let folder = folder
                .as_deref()
                .and_then(|path| std::path::Path::new(path).file_name())
                .and_then(|name| name.to_str());
            if let Some(folder) = folder {
                project["folder"] = json!(folder);
            }
            project
        })
        .collect())
}

/// Every task's name and project, by workspace id; the homes are left out.
fn task_labels(conn: &Connection) -> Result<HashMap<String, (String, String)>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT w.id,
                COALESCE(NULLIF(w.name, ''), (SELECT t.title FROM dcc_sessions s
                   JOIN dcc_threads t ON t.session_id = s.id
                   WHERE s.workspace_id = w.id LIMIT 1), w.root_path),
                COALESCE(r.display_name, r.name, w.root_path)
         FROM dcc_workspaces w LEFT JOIN dcc_repositories r ON r.project_id = w.project_id
         WHERE w.project_id NOT LIKE 'dcc-agent-%'",
    )?;
    let labels = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(labels)
}

fn reason_name(reason: AttentionReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// What needs the person: the attention list without the new results (the
/// sidebar drops those once seen, and `recent` already has them), or the
/// tasks waiting on them when the list could not be read.
fn needs_you(
    attention: Option<&[AttentionItem]>,
    labels: &HashMap<String, (String, String)>,
    tasks: &[Task],
) -> Vec<Value> {
    let Some(attention) = attention else {
        return tasks
            .iter()
            .filter(|task| matches!(task.status.as_str(), "permission" | "input"))
            .map(|task| {
                json!({
                    "task": task.title,
                    "project": task.project,
                    "reason": task.status,
                    "since": local_time(&task.updated_at),
                })
            })
            .collect();
    };
    attention
        .iter()
        .filter(|item| {
            !matches!(
                item.reason,
                AttentionReason::Completed | AttentionReason::Aborted
            )
        })
        .filter_map(|item| {
            let (task, project) = labels.get(&item.workspace_id)?;
            let mut view =
                json!({"task": task, "project": project, "reason": reason_name(item.reason)});
            match item.reason {
                AttentionReason::ChecksFailing | AttentionReason::PrConflicts => {
                    view["pr"] = json!(item.count);
                }
                AttentionReason::DelegatedReview => view["count"] = json!(item.count),
                _ => {}
            }
            if let Some(since) = &item.returned_at {
                view["since"] = json!(local_time(since));
            }
            Some(view)
        })
        .collect()
}

/// The name of `project_id` as the overview lists projects.
fn project_label(conn: &Connection, project_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT MIN(COALESCE(NULLIF(display_name, ''), name)) FROM dcc_repositories WHERE project_id = ?1",
        [project_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

/// In a project's room, its items first (the order stays otherwise).
fn room_first(items: &mut [Value], room: &str) {
    items.sort_by_key(|item| item.get("project").and_then(Value::as_str) != Some(room));
}

/// The overview as the agent reads it. `attention` is DCC's "Precisa de
/// você" list, or `None` when it could not be read; `room` is the project
/// whose room the agent talks in; `only` the projects chosen for it.
pub fn read_overview(
    db_path: &std::path::Path,
    attention: Option<&[AttentionItem]>,
    room: Option<&str>,
    only: &[String],
) -> Result<Value, String> {
    let mut conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_millis(300))
        .map_err(|error| error.to_string())?;
    let tasks = read_tasks_with(&mut conn, RECENT_TASKS).map_err(|error| error.to_string())?;
    let projects = projects(&conn, only).map_err(|error| error.to_string())?;
    let labels = task_labels(&conn).map_err(|error| error.to_string())?;
    let pick = |statuses: &[&str]| -> Vec<Value> {
        tasks
            .iter()
            .filter(|task| statuses.contains(&task.status.as_str()))
            .map(task_view)
            .collect()
    };
    let mut needs = needs_you(attention, &labels, &tasks);
    let mut running = pick(&["running"]);
    let mut recent = pick(&["completed", "aborted"]);
    let room = room.and_then(|project| project_label(&conn, project));
    if let Some(room) = room.as_deref() {
        for items in [&mut needs, &mut running, &mut recent] {
            room_first(items, room);
        }
    }
    let mut overview = json!({
        "now": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        "projects": projects,
        "needsYou": needs,
        "running": running,
        "recent": recent,
    });
    if let Some(room) = room {
        overview["room"] = json!(room);
    }
    Ok(overview)
}

pub async fn dispatch(sessions: &SessionCommandState, session_id: &str) -> Value {
    match sessions.is_agent_conversation(&SessionId(session_id.to_string())) {
        Ok(true) => {}
        Ok(false) => {
            return text(
                "dcc_overview only works in a resident agent's own conversation.".to_string(),
                true,
            )
        }
        Err(error) => return text(error.to_string(), true),
    }
    let workspace = WorkspaceCommandState::from_session(sessions);
    let attention = workspace_attention_with_state(&workspace, BLOCKERS_MAX_AGE)
        .await
        .ok()
        .map(|output| {
            output
                .items
                .into_iter()
                .map(|entry| entry.item)
                .collect::<Vec<_>>()
        });
    let db_path = sessions.database_file().to_path_buf();
    // In a project's room, its project first.
    let room = sessions
        .session_room(&SessionId(session_id.to_string()))
        .ok()
        .flatten();
    let only = sessions
        .session_agent(&SessionId(session_id.to_string()))
        .ok()
        .flatten()
        .map(|agent| agent.projects)
        .unwrap_or_default();
    let overview = tokio::task::spawn_blocking(move || {
        read_overview(&db_path, attention.as_deref(), room.as_deref(), &only)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    // The tasks it opened from any room, with ids to continue them here.
    let own = crate::overview_own_tasks::own_open_tasks(sessions, session_id).await;
    let overview = overview.map(|mut overview| {
        if !own.is_empty() {
            overview["yourOpenTasks"] = json!(own);
        }
        overview
    });
    match overview {
        Ok(overview) => text(
            serde_json::to_string_pretty(&overview).unwrap_or_default(),
            false,
        ),
        Err(error) => text(format!("Could not read DCC's tasks: {error}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rooms_project_comes_first() {
        let mut items = vec![
            json!({"task": "a", "project": "OmniFast"}),
            json!({"task": "b", "project": "Checkout"}),
            json!({"task": "c", "project": "OmniFast"}),
        ];
        room_first(&mut items, "Checkout");
        let order: Vec<&str> = items
            .iter()
            .map(|item| item["task"].as_str().unwrap())
            .collect();
        assert_eq!(order, ["b", "a", "c"]);
    }

    #[test]
    fn the_overview_takes_no_arguments_and_skips_the_homes() {
        assert!(call_is_well_formed(&None));
        assert!(call_is_well_formed(&Some(json!({}))));
        assert!(!call_is_well_formed(&Some(json!({"project": "x"}))));
        assert_eq!(definition()["name"], OVERVIEW_TOOL);

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE dcc_sessions (id TEXT, workspace_id TEXT);
            CREATE TABLE dcc_threads (session_id TEXT, title TEXT, archived_at TEXT);
            CREATE TABLE dcc_workspaces (id TEXT, project_id TEXT, name TEXT, root_path TEXT, state TEXT);
            CREATE TABLE dcc_repositories (project_id TEXT, display_name TEXT, name TEXT, root_path TEXT, remote_url TEXT);
            CREATE TABLE dcc_session_events (session_id TEXT, sequence INTEGER, kind_json TEXT, occurred_at TEXT);
            INSERT INTO dcc_repositories VALUES ('p', 'Loja', 'loja', '/code/loja-web', 'https://someone:secret@gitlab.acme.com/acme/loja-web.git');
            INSERT INTO dcc_repositories VALUES ('q', NULL, 'painel', '/code/painel', 'git@github.com:outra/painel.git');
            INSERT INTO dcc_repositories VALUES ('dcc-agent-a', NULL, 'a', '/home/a', NULL);
            INSERT INTO dcc_workspaces VALUES ('w', 'p', 'Checkout', '/loja', 'ready');
            INSERT INTO dcc_workspaces VALUES ('h', 'dcc-agent-a', 'Conversa', '/home/a', 'ready');
            INSERT INTO dcc_sessions VALUES ('s', 'w');
            INSERT INTO dcc_sessions VALUES ('sh', 'h');
            INSERT INTO dcc_threads VALUES ('s', 'Checkout', NULL);
            INSERT INTO dcc_threads VALUES ('sh', 'Conversa', NULL);
            INSERT INTO dcc_session_events VALUES ('s', 1, '{\"type\":\"turn_started\",\"turnId\":\"t\"}', '2026-11-06T10:00:00Z');
            INSERT INTO dcc_session_events VALUES ('sh', 1, '{\"type\":\"turn_started\",\"turnId\":\"t\"}', '2026-11-06T10:00:00Z');",
        )
        .unwrap();
        let item = |workspace: &str, reason, count| AttentionItem {
            workspace_id: workspace.to_string(),
            reason,
            count,
            returned_at: Some("2026-11-06T10:05:00Z".to_string()),
            completed_at: None,
        };
        let attention = [
            item("w", AttentionReason::ChecksFailing, Some(1032)),
            item("w", AttentionReason::Completed, None),
            item("h", AttentionReason::Input, None),
        ];
        let overview = read_overview(&db, Some(&attention), None, &[]).unwrap();
        assert!(overview.get("room").is_none());
        // Each project says where its code lives, with no user or password.
        assert_eq!(
            overview["projects"],
            json!([
                {"name": "Loja", "repository": "gitlab.acme.com/acme/loja-web", "folder": "loja-web"},
                {"name": "painel", "repository": "github.com/outra/painel", "folder": "painel"}
            ])
        );
        // An agent placed in some projects sees only those.
        let placed = read_overview(&db, None, None, &["p".to_string()]).unwrap();
        assert_eq!(placed["projects"].as_array().unwrap().len(), 1);
        assert_eq!(placed["projects"][0]["name"], "Loja");
        assert_eq!(overview["running"][0]["task"], "Checkout");
        assert_eq!(overview["running"].as_array().unwrap().len(), 1);
        let needs = overview["needsYou"].as_array().unwrap();
        assert_eq!(needs.len(), 1);
        assert_eq!(needs[0]["task"], "Checkout");
        assert_eq!(needs[0]["project"], "Loja");
        assert_eq!(needs[0]["reason"], "checksFailing");
        assert_eq!(needs[0]["pr"], 1032);
        let since = needs[0]["since"].as_str().unwrap();
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(since).unwrap(),
            chrono::DateTime::parse_from_rfc3339("2026-11-06T10:05:00Z").unwrap()
        );
        assert!(chrono::DateTime::parse_from_rfc3339(
            overview["running"][0]["updatedAt"].as_str().unwrap()
        )
        .is_ok());

        let fallback = read_overview(&db, None, None, &[]).unwrap();
        assert_eq!(fallback["needsYou"], json!([]));
        // In the room of project "p": it says so.
        let in_room = read_overview(&db, None, Some("p"), &[]).unwrap();
        assert_eq!(in_room["room"], "Loja");
    }
}
