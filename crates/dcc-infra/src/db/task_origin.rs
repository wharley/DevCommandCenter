//! A task's origin read from the database (`dcc_core::domain::task_origin`):
//! the delegation record that opened it, the turn of the agent's
//! conversation that asked, and — while that turn was a teammate's pass —
//! the teammate's own latest turn before it. Read only; nothing is stored
//! for it.

use super::*;
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::task_origin::{started, OriginStep, Started, TaskOrigin, MAX_PASSES};

fn repository_error(error: impl ToString) -> dcc_core::CoreError {
    dcc_core::CoreError::Repository(error.to_string())
}

/// A turn's start: its session, when, and its prompt.
struct Turn {
    session_id: String,
    at: String,
    prompt: String,
}

fn turn_from_row(row: &Row<'_>) -> rusqlite::Result<Turn> {
    Ok(Turn {
        session_id: row.get(0)?,
        at: row.get(1)?,
        prompt: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
    })
}

const TURN_COLUMNS: &str = "session_id, occurred_at, json_extract(kind_json, '$.prompt')";

impl SqliteSessionRepo {
    /// Where the task `workspace_id` came from, when an agent opened it.
    pub fn task_origin(&self, workspace_id: &str) -> Result<Option<TaskOrigin>> {
        let conn = self.conn.lock().map_err(repository_error)?;
        // The record of the task's first conversation an agent opened.
        let opened: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT d.parent_session_id, d.parent_turn_id, d.created_at
                   FROM dcc_delegations d JOIN dcc_sessions s ON s.id = d.child_session_id
                  WHERE s.workspace_id = ?1 AND d.kind = 'task'
                  ORDER BY d.created_at LIMIT 1",
                params![workspace_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(repository_error)?;
        let Some((parent, turn_id, opened_at)) = opened else {
            return Ok(None);
        };
        // The turn that asked: the one named, or the latest before the task.
        let named = match &turn_id {
            Some(turn_id) => conn
                .query_row(
                    &format!(
                        "SELECT {TURN_COLUMNS} FROM dcc_session_events
                          WHERE session_id = ?1
                            AND json_extract(kind_json, '$.type') = 'turn_started'
                            AND json_extract(kind_json, '$.turnId') = ?2 LIMIT 1"
                    ),
                    params![parent, turn_id],
                    turn_from_row,
                )
                .optional()
                .map_err(repository_error)?,
            None => None,
        };
        let turn = match named {
            Some(turn) => Some(turn),
            None => conn
                .query_row(
                    &format!(
                        "SELECT {TURN_COLUMNS} FROM dcc_session_events
                          WHERE session_id = ?1
                            AND json_extract(kind_json, '$.type') = 'turn_started'
                            AND occurred_at <= ?2
                          ORDER BY occurred_at DESC LIMIT 1"
                    ),
                    params![parent, opened_at],
                    turn_from_row,
                )
                .optional()
                .map_err(repository_error)?,
        };
        let Some(mut turn) = turn else {
            return Ok(None);
        };
        let agent_of = |session_id: &str| -> Result<Option<(String, String)>> {
            conn.query_row(
                "SELECT a.id, a.name FROM dcc_session_agents b JOIN dcc_agents a ON a.id = b.agent_id
                  WHERE b.session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(repository_error)
        };
        let room_of = |session_id: &str| -> Result<Option<String>> {
            conn.query_row(
                "SELECT project_id FROM dcc_agent_rooms WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(repository_error)
        };
        let mut steps: Vec<OriginStep> = Vec::new();
        for _ in 0..=MAX_PASSES {
            let Some((agent_id, agent)) = agent_of(&turn.session_id)? else {
                break;
            };
            let what = started(&turn.prompt);
            let from = match &what {
                Started::Passed { from, .. } => Some(from.clone()),
                _ => None,
            };
            steps.push(OriginStep {
                started: what,
                agent_id,
                agent,
                room: room_of(&turn.session_id)?,
                session_id: turn.session_id.clone(),
                at: turn.at.clone(),
            });
            // A pass: the teammate's own latest turn before it, in its own
            // conversations.
            let Some(from) = from else {
                break;
            };
            let teammate: Option<String> = conn
                .query_row(
                    "SELECT id FROM dcc_agents WHERE name = ?1 AND deleted_at IS NULL LIMIT 1",
                    params![from],
                    |row| row.get(0),
                )
                .optional()
                .map_err(repository_error)?;
            let Some(teammate) = teammate else {
                break;
            };
            let before = conn
                .query_row(
                    "SELECT e.session_id, e.occurred_at, json_extract(e.kind_json, '$.prompt')
                           FROM dcc_session_events e
                           JOIN dcc_sessions s ON s.id = e.session_id
                           JOIN dcc_session_agents b ON b.session_id = s.id
                          WHERE b.agent_id = ?1 AND s.project_id = ?2
                            AND json_extract(e.kind_json, '$.type') = 'turn_started'
                            AND e.occurred_at <= ?3
                          ORDER BY e.occurred_at DESC LIMIT 1",
                    params![teammate, home_project_id(&teammate), turn.at],
                    turn_from_row,
                )
                .optional()
                .map_err(repository_error)?;
            match before {
                Some(before) => turn = before,
                None => break,
            }
        }
        if steps.is_empty() {
            return Ok(None);
        }
        steps.reverse();
        Ok(Some(TaskOrigin { steps, opened_at }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent_source::{source_prompt, AgentSource, SourceItem};
    use dcc_core::domain::team_room::handoff_prompt;

    /// The walk reads a few columns: the rows are written without the
    /// projects and tasks the real ones hang from.
    fn seed(repo: &SqliteSessionRepo) {
        let conn = repo.conn.lock().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    }

    fn session(conn: &Connection, id: &str, project: &str, workspace: &str) {
        conn.execute(
            "INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'codex', 'idle', '2026-11-14T09:00:00Z', '2026-11-14T09:00:00Z')",
            params![id, project, workspace],
        )
        .unwrap();
    }

    fn agent(conn: &Connection, id: &str, name: &str, home: &str) {
        conn.execute(
            "INSERT INTO dcc_agents (id, name, role, avatar_color, avatar_arms, avatar_eyes, created_at, updated_at)
             VALUES (?1, ?2, 'r', 'purple', 2, 'round', 't', 't')",
            params![id, name],
        )
        .unwrap();
        session(conn, home, &home_project_id(id), "w-home");
        conn.execute(
            "INSERT INTO dcc_session_agents (session_id, agent_id, created_at) VALUES (?1, ?2, 't')",
            params![home, id],
        )
        .unwrap();
    }

    fn turn(
        conn: &Connection,
        session: &str,
        sequence: i64,
        turn_id: &str,
        at: &str,
        prompt: &str,
    ) {
        let kind = serde_json::json!({"type": "turn_started", "turnId": turn_id, "prompt": prompt});
        conn.execute(
            "INSERT INTO dcc_session_events (event_id, session_id, sequence, occurred_at, kind_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![format!("{session}-{sequence}"), session, sequence, at, kind.to_string()],
        )
        .unwrap();
    }

    #[test]
    fn a_task_is_followed_back_through_the_pass_to_the_sources_news() {
        let dir = tempfile::tempdir().unwrap();
        let repo = SqliteSessionRepo::open(dir.path().join("database.sqlite")).unwrap();
        seed(&repo);
        {
            let conn = repo.conn.lock().unwrap();
            agent(&conn, "sup", "Suporte", "s-sup");
            agent(&conn, "dev", "Dev", "s-dev");
            conn.execute(
                "INSERT INTO dcc_agent_rooms (session_id, project_id, created_at) VALUES ('s-dev', 'checkout', 't')",
                [],
            )
            .unwrap();
            let source = AgentSource {
                id: "src".into(),
                agent_id: "sup".into(),
                connector_id: "slack".into(),
                connector: "Slack".into(),
                watch: "menções a mim".into(),
                tool: "slack_search_public".into(),
                arguments: "{}".into(),
                every_minutes: 15,
                enabled: true,
                proposed: false,
                last_checked_at: None,
                created_at: String::new(),
                room: None,
            };
            let news = [SourceItem {
                id: "1".into(),
                text: "{\"text\":\"o CEP não calcula frete\",\"permalink\":\"https://acme.slack.com/p1\"}".into(),
            }];
            // Suporte: an older turn the person wrote, then the source's news.
            turn(
                &conn,
                "s-sup",
                1,
                "t-old",
                "2026-11-14T08:00:00Z",
                "bom dia",
            );
            turn(
                &conn,
                "s-sup",
                2,
                "t-src",
                "2026-11-14T09:15:00Z",
                &source_prompt(&source, &news),
            );
            // Dev: the pass from Suporte, whose turn opened the task.
            turn(
                &conn,
                "s-dev",
                1,
                "t-pass",
                "2026-11-14T09:16:00Z",
                &handoff_prompt("Suporte", "Dev", "corrige o CEP", "O campo não valida."),
            );
            // The task, its conversation, and the record that opened it.
            session(&conn, "s-task", "p-loja", "w-task");
            conn.execute(
                "INSERT INTO dcc_delegations (id, parent_session_id, parent_turn_id, child_session_id, workspace_id,
                    target_provider_id, mode, status, prompt, context_policy_json, budget_json, created_at, updated_at,
                    origin, kind)
                 VALUES ('d-1', 's-dev', 't-pass', 's-task', 'w-home', 'codex', 'write', 'completed', 'p', '{}', '{}',
                    '2026-11-14T09:17:00Z', '2026-11-14T09:17:00Z', 'agent', 'task')",
                [],
            )
            .unwrap();
        }
        let origin = repo.task_origin("w-task").unwrap().unwrap();
        assert_eq!(origin.opened_at, "2026-11-14T09:17:00Z");
        assert_eq!(origin.steps.len(), 2);
        // Oldest first: the source woke Suporte in Geral…
        let first = &origin.steps[0];
        assert_eq!((first.agent.as_str(), &first.room), ("Suporte", &None));
        assert_eq!(
            first.started,
            Started::Source {
                connector: "Slack".into(),
                watch: "menções a mim".into(),
                excerpt: "o CEP não calcula frete".into(),
                link: Some("https://acme.slack.com/p1".into()),
            }
        );
        // …and Suporte passed it to Dev, in the Checkout's room, who opened it.
        let last = &origin.steps[1];
        assert_eq!(
            (last.agent.as_str(), last.room.as_deref()),
            ("Dev", Some("checkout"))
        );
        assert!(matches!(&last.started, Started::Passed { from, .. } if from == "Suporte"));
        // A task the person made has no origin.
        assert_eq!(repo.task_origin("w-home").unwrap(), None);
        assert_eq!(repo.task_origin("nope").unwrap(), None);
    }
}
