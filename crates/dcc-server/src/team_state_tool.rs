//! `dcc_team_state`: in a resident agent's own conversation, how the team is
//! set up — each agent's model, projects, triggers, what it sends without
//! asking, its sources with the room each brings news to, its routines —
//! and what in it does not close (`agent_grants_check`, the rules of the
//! desktop's "Conferir"). It is what answers "is this set up right?" and
//! "why did nothing happen?" without the person reading settings screen by
//! screen. Settings and switches only: never what was said in a
//! conversation, and nothing here changes anything. Read from the database
//! and the desktop's own files, with no model.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use dcc_core::domain::agent_conversation::TEAM_STATE_TOOL;
use dcc_core::domain::agent_grants_check::{gaps, stored_grants, Grants, GENERAL_ROOM};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

/// The desktop's switches and grants, beside the database.
const TRIGGERS_FILE: &str = "agent-triggers.json";
/// The reviews at work now, beside the database.
const REVIEWS_FILE: &str = "pr-reviews-running.json";

pub fn is_team_state_tool(name: &str) -> bool {
    name == TEAM_STATE_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": TEAM_STATE_TOOL,
        "description": "Only in your own conversation as a DCC resident agent: how the person's team of agents is set up, read from what is stored. For each agent: its model, the projects it works in (all, or the chosen ones), whether it starts tasks by itself, who it may pass work to, the triggers turned on, what it sends without asking and where (reviews, replies, a task's delivery, the announcement, fixing in the task), its sources (connector, what each watches, how often, the room it brings news to, when it last read) and its routines; notClosing lists what is set that will not do what the person expects, and notes what follows from where a source is. Call it before saying whether the setup is right or why something did not happen, and answer from it, not from memory. It shows settings and switches, never what was said in a conversation; it changes nothing. A task an event opens starts by itself only in a project chosen for the agent or in the project of the room the turn runs in; elsewhere it waits as a card.",
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

/// A JSON file beside the database; null when it is not there or not JSON.
fn beside(db_path: &Path, name: &str) -> Value {
    db_path
        .parent()
        .and_then(|dir| std::fs::read_to_string(dir.join(name)).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

fn string_list(json: Option<String>) -> Vec<String> {
    json.and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default()
}

/// Every project's name, by id; the agents' homes are left out.
fn project_names(conn: &Connection) -> Result<HashMap<String, String>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT project_id, MIN(COALESCE(NULLIF(display_name, ''), name))
         FROM dcc_repositories WHERE project_id NOT LIKE 'dcc-agent-%' GROUP BY project_id",
    )?;
    let names = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect();
    names
}

struct Agent {
    id: String,
    name: String,
    role: String,
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    projects: Vec<String>,
    may_call: Vec<String>,
    starts_tasks: bool,
}

fn agents(conn: &Connection) -> Result<Vec<Agent>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, name, role, provider_id, model, effort, projects_json, may_call_json,
                auto_start_tasks
         FROM dcc_agents WHERE deleted_at IS NULL ORDER BY created_at",
    )?;
    let agents = statement
        .query_map([], |row| {
            Ok(Agent {
                id: row.get(0)?,
                name: row.get(1)?,
                role: row.get(2)?,
                provider: row.get(3)?,
                model: row.get(4)?,
                effort: row.get(5)?,
                projects: string_list(row.get(6)?),
                may_call: string_list(row.get(7)?),
                starts_tasks: row.get::<_, i64>(8)? != 0,
            })
        })?
        .collect();
    agents
}

/// The room a grant or a source names: Geral, or the project's name.
fn room_name(room: &str, projects: &HashMap<String, String>) -> String {
    if room == GENERAL_ROOM {
        "Geral".to_string()
    } else {
        projects
            .get(room)
            .cloned()
            .unwrap_or_else(|| room.to_string())
    }
}

/// An agent's sources as it reads them, and the room of each.
fn sources(
    conn: &Connection,
    agent_id: &str,
    projects: &HashMap<String, String>,
) -> Result<(Vec<Value>, Vec<Option<String>>), rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT connector, watch, every_minutes, enabled, proposed, room_project_id,
                last_checked_at
         FROM dcc_agent_sources WHERE agent_id = ?1 ORDER BY created_at",
    )?;
    let rows = statement
        .query_map([agent_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut views = Vec::new();
    let mut rooms = Vec::new();
    for (connector, watch, every, enabled, proposed, room, last) in rows {
        let room = room.filter(|room| !room.is_empty());
        let mut view = json!({
            "connector": connector,
            "watches": watch,
            "everyMinutes": every,
            "on": enabled && !proposed,
            "room": room_name(room.as_deref().unwrap_or(GENERAL_ROOM), projects),
        });
        if proposed {
            view["waitsForThePerson"] = json!(true);
        }
        match last {
            Some(last) => view["lastRead"] = json!(crate::overview_tool::local_time(&last)),
            None => view["lastRead"] = json!("never"),
        }
        if enabled && !proposed {
            rooms.push(room);
        }
        views.push(view);
    }
    Ok((views, rooms))
}

fn routines(conn: &Connection, agent_id: &str) -> Result<Vec<Value>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT days, hour, minute, enabled, proposed, last_run_at
         FROM dcc_agent_routines WHERE agent_id = ?1 ORDER BY hour, minute",
    )?;
    let routines = statement
        .query_map([agent_id], |row| {
            let (hour, minute): (i64, i64) = (row.get(1)?, row.get(2)?);
            let proposed = row.get::<_, i64>(4)? != 0;
            let mut view = json!({
                "days": row.get::<_, String>(0)?,
                "time": format!("{hour:02}:{minute:02}"),
                "on": row.get::<_, i64>(3)? != 0 && !proposed,
            });
            if let Some(last) = row.get::<_, Option<String>>(5)? {
                view["lastRun"] = json!(crate::overview_tool::local_time(&last));
            }
            Ok(view)
        })?
        .collect();
    routines
}

/// What the agent sends without asking, with the places by name.
fn sends(
    grants: &Grants,
    file: &Value,
    agent_id: &str,
    projects: &HashMap<String, String>,
) -> Value {
    let named =
        |ids: &[String]| -> Vec<String> { ids.iter().map(|id| room_name(id, projects)).collect() };
    let mut sends = json!({});
    if grants.review {
        sends["reviews"] = json!({
            "how": file["autoSend"][agent_id]["review"],
            "projects": named(&grants.review_projects),
        });
    }
    if grants.reply {
        sends["replies"] = json!({"rooms": named(&grants.reply_rooms)});
    }
    if grants.deliver {
        sends["taskDelivery"] = json!({
            "projects": named(&grants.deliver_projects),
            "announcesThePullRequest": grants.announces,
        });
    }
    if grants.fix_in_task {
        sends["reviewCommentsGoToTheTask"] = json!(true);
    }
    sends
}

/// What follows from where the agent's sources are.
fn source_notes(
    agent: &Agent,
    rooms: &[Option<String>],
    projects: &HashMap<String, String>,
) -> Vec<String> {
    let mut notes = Vec::new();
    if rooms.iter().any(Option::is_none) && agent.projects.is_empty() {
        notes.push("A source of its brings news to Geral, which has no project, and no project is chosen for it: a task opened from that news waits as a card instead of starting. Moving the source to the project's room is what says where its requests belong.".to_string());
    }
    for room in rooms.iter().flatten() {
        if !agent.projects.is_empty() && !agent.projects.contains(room) {
            notes.push(format!(
                "A source of its is in the room of {}, which is not among its chosen projects.",
                room_name(room, projects)
            ));
        }
    }
    notes.dedup();
    notes
}

/// The team's state as an agent reads it.
pub fn read_team_state(db_path: &Path) -> Result<Value, String> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    conn.busy_timeout(Duration::from_millis(300))
        .map_err(|error| error.to_string())?;
    let file = beside(db_path, TRIGGERS_FILE);
    let projects = project_names(&conn).map_err(|error| error.to_string())?;
    let team = agents(&conn).map_err(|error| error.to_string())?;
    let names: HashMap<&str, &str> = team
        .iter()
        .map(|agent| (agent.id.as_str(), agent.name.as_str()))
        .collect();
    let mut views = Vec::new();
    for agent in &team {
        let grants = stored_grants(&file, &agent.id);
        let (sources, rooms) =
            sources(&conn, &agent.id, &projects).map_err(|error| error.to_string())?;
        let routines = routines(&conn, &agent.id).map_err(|error| error.to_string())?;
        let mut view = json!({
            "name": agent.name,
            "role": agent.role,
            "projects": if agent.projects.is_empty() {
                json!("all")
            } else {
                json!(agent.projects.iter().map(|id| room_name(id, &projects)).collect::<Vec<_>>())
            },
            "startsTasksByItself": agent.starts_tasks,
            "mayPassWorkTo": agent
                .may_call
                .iter()
                .filter_map(|id| names.get(id.as_str()).copied())
                .collect::<Vec<_>>(),
            "triggersOn": grants.triggers.iter().map(|trigger| trigger.id()).collect::<Vec<_>>(),
            "sendsWithoutAsking": sends(&grants, &file, &agent.id, &projects),
            "sources": sources,
            "routines": routines,
            "notClosing": gaps(&grants, &rooms).into_iter().map(|gap| gap.says()).collect::<Vec<_>>(),
            "notes": source_notes(agent, &rooms, &projects),
        });
        let model = [&agent.provider, &agent.model, &agent.effort]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" · ");
        if !model.is_empty() {
            view["model"] = json!(model);
        }
        if let Some(ceiling) = file["ceilings"][&agent.id]
            .as_u64()
            .filter(|ceiling| *ceiling > 0)
        {
            view["triggerRunsADay"] = json!(ceiling);
        }
        views.push(view);
    }
    let mut state = json!({
        "now": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        "allPaused": file["paused"].as_bool().unwrap_or(false),
        "agents": views,
        "reviewsAtWorkNow": beside(db_path, REVIEWS_FILE).as_array().map_or(0, Vec::len),
    });
    if !file["quiet"].is_null() {
        state["quietHours"] = file["quiet"].clone();
    }
    if file.is_null() {
        state["switchesUnread"] = json!("The desktop's switches could not be read: triggers and what goes without asking show as off.");
    }
    Ok(state)
}

pub async fn dispatch(sessions: &SessionCommandState, session_id: &str) -> Value {
    match sessions.is_agent_conversation(&SessionId(session_id.to_string())) {
        Ok(true) => {}
        Ok(false) => {
            return text(
                "dcc_team_state only works in a resident agent's own conversation.".to_string(),
                true,
            )
        }
        Err(error) => return text(error.to_string(), true),
    }
    let db_path = sessions.database_file().to_path_buf();
    let state = tokio::task::spawn_blocking(move || read_team_state(&db_path))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
    match state {
        Ok(state) => text(state.to_string(), false),
        Err(error) => text(format!("the team's state could not be read: {error}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_team_reads_as_it_is_set_with_what_does_not_close() {
        assert!(call_is_well_formed(&None) && call_is_well_formed(&Some(json!({}))));
        assert!(!call_is_well_formed(&Some(json!({"agent": "Dev"}))));
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("database.sqlite");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE dcc_repositories (project_id TEXT, display_name TEXT, name TEXT);
            CREATE TABLE dcc_agents (id TEXT, name TEXT, role TEXT, provider_id TEXT, model TEXT, effort TEXT, projects_json TEXT, may_call_json TEXT, auto_start_tasks INTEGER, deleted_at TEXT, created_at TEXT);
            CREATE TABLE dcc_agent_sources (agent_id TEXT, connector TEXT, watch TEXT, every_minutes INTEGER, enabled INTEGER, proposed INTEGER, room_project_id TEXT, last_checked_at TEXT, created_at TEXT);
            CREATE TABLE dcc_agent_routines (agent_id TEXT, days TEXT, hour INTEGER, minute INTEGER, enabled INTEGER, proposed INTEGER, last_run_at TEXT);
            INSERT INTO dcc_repositories VALUES ('app', 'Checkout', 'vendeagora-app');
            INSERT INTO dcc_repositories VALUES ('dcc-agent-sup', NULL, 'sup');
            INSERT INTO dcc_agents VALUES ('sup', 'Suporte', 'Suporte', 'codex', 'gpt', 'medium', NULL, '[\"dev\"]', 0, NULL, '1');
            INSERT INTO dcc_agents VALUES ('dev', 'Dev', 'Dev', NULL, NULL, NULL, '[\"app\"]', NULL, 1, NULL, '2');
            INSERT INTO dcc_agents VALUES ('old', 'Repórter', 'x', NULL, NULL, NULL, NULL, NULL, 0, '2026-01-01', '3');
            INSERT INTO dcc_agent_sources VALUES ('sup', 'Slack', 'menções a mim', 1, 1, 0, NULL, NULL, '1');
            INSERT INTO dcc_agent_routines VALUES ('dev', 'mon,tue', 9, 5, 1, 0, NULL);",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(TRIGGERS_FILE),
            json!({
                "paused": false,
                "on": {"dev": ["ciFailed"]},
                "ceilings": {"dev": 10},
                "autoSend": {
                    "sup": {"reply": true, "replyRooms": ["app"]},
                    "dev": {"deliver": true, "deliverProjects": ["app"], "deliverNote": "avise"},
                }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(dir.path().join(REVIEWS_FILE), "[{\"prId\":\"x\"}]").unwrap();

        let state = read_team_state(&db).unwrap();
        assert_eq!(state["allPaused"], false);
        assert_eq!(state["reviewsAtWorkNow"], 1);
        let agents = state["agents"].as_array().unwrap();
        // An agent the person removed is not on the team.
        assert_eq!(agents.len(), 2);
        let sup = &agents[0];
        assert_eq!(sup["name"], "Suporte");
        assert_eq!(sup["model"], "codex · gpt · medium");
        assert_eq!(sup["projects"], "all");
        assert_eq!(sup["mayPassWorkTo"], json!(["Dev"]));
        assert_eq!(sup["sources"][0]["room"], "Geral");
        assert_eq!(sup["sources"][0]["lastRead"], "never");
        assert_eq!(
            sup["sendsWithoutAsking"]["replies"]["rooms"],
            json!(["Checkout"])
        );
        // Its replies are granted in Checkout and its source is in Geral.
        assert_eq!(sup["notClosing"].as_array().unwrap().len(), 1);
        assert!(sup["notClosing"][0].as_str().unwrap().contains("source"));
        assert!(sup["notes"][0].as_str().unwrap().contains("Geral"));
        let dev = &agents[1];
        assert_eq!(dev["projects"], json!(["Checkout"]));
        assert_eq!(dev["startsTasksByItself"], true);
        assert_eq!(dev["triggersOn"], json!(["ciFailed"]));
        assert_eq!(dev["triggerRunsADay"], 10);
        assert_eq!(dev["routines"][0]["time"], "09:05");
        assert_eq!(
            dev["sendsWithoutAsking"]["taskDelivery"]["announcesThePullRequest"],
            true
        );
        // The announcement is written and its replies are not granted.
        assert!(dev["notClosing"][0]
            .as_str()
            .unwrap()
            .contains("Ao abrir o PR"));
        assert!(dev.get("model").is_none());

        // With no switches file, everything shows as off, and it says so.
        std::fs::remove_file(dir.path().join(TRIGGERS_FILE)).unwrap();
        let bare = read_team_state(&db).unwrap();
        assert!(bare["switchesUnread"].is_string());
        assert_eq!(bare["agents"][1]["sendsWithoutAsking"], json!({}));
    }
}
