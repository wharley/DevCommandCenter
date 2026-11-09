//! The agents' week for the bulletin (`dcc_core::domain::agent_bulletin`):
//! their turns that finished or stopped and the tasks they worked in, from
//! the turn ends' partial indexes on `occurred_at` (like the daily recap),
//! and their tokens and reported cost, from the usage table's time index —
//! two grouped reads for every agent at once, run when the bulletin opens.

use super::*;
use dcc_core::domain::agent_budget::counted_tokens;
use dcc_core::domain::agent_bulletin::AgentWeek;
use dcc_core::domain::agent_reaction::Reaction;

impl SqliteSessionRepo {
    /// Every agent that did something between `from` and `to`.
    pub fn agents_week(&self, from: &str, to: &str) -> Result<Vec<AgentWeek>> {
        let conn = self
            .conn
            .lock()
            .map_err(|error| dcc_core::CoreError::Repository(error.to_string()))?;
        let repository =
            |error: rusqlite::Error| dcc_core::CoreError::Repository(error.to_string());
        let mut weeks: HashMap<String, AgentWeek> = HashMap::new();
        let mut tasks: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
        let mut turns = conn
            .prepare(
                "SELECT b.agent_id, e.kind, s.workspace_id, s.project_id
                 FROM (
                     SELECT session_id, 'completed' AS kind FROM dcc_session_events
                     WHERE json_extract(kind_json, '$.type') = 'turn_completed'
                       AND occurred_at >= ?1 AND occurred_at < ?2
                     UNION ALL
                     SELECT session_id, 'aborted' AS kind FROM dcc_session_events
                     WHERE json_extract(kind_json, '$.type') = 'turn_aborted'
                       AND occurred_at >= ?1 AND occurred_at < ?2
                 ) e
                 JOIN dcc_session_agents b ON b.session_id = e.session_id
                 JOIN dcc_sessions s ON s.id = e.session_id",
            )
            .map_err(repository)?;
        let rows = turns
            .query_map(params![from, to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository)?;
        for (agent_id, kind, workspace, project) in rows {
            let week = weeks.entry(agent_id.clone()).or_insert_with(|| AgentWeek {
                agent_id: agent_id.clone(),
                ..AgentWeek::default()
            });
            if kind == "completed" {
                week.answers += 1;
            } else {
                week.stopped += 1;
            }
            if !project.starts_with(dcc_core::domain::agent_conversation::HOME_PROJECT_PREFIX) {
                tasks.entry(agent_id).or_default().insert(workspace);
            }
        }
        let mut usage = conn
            .prepare(
                "SELECT b.agent_id, u.total_tokens, u.cached_input_tokens, u.cost_usd
                 FROM dcc_turn_model_usage u
                 JOIN dcc_session_agents b ON b.session_id = u.session_id
                 WHERE u.recorded_at >= ?1 AND u.recorded_at < ?2",
            )
            .map_err(repository)?;
        let rows = usage
            .query_map(params![from, to], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                ))
            })
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .map_err(repository)?;
        for (agent_id, total, cached, cost) in rows {
            let week = weeks.entry(agent_id.clone()).or_insert_with(|| AgentWeek {
                agent_id,
                ..AgentWeek::default()
            });
            week.tokens += counted_tokens(total.max(0) as u64, cached.max(0) as u64);
            if let Some(cost) = cost {
                *week.cost_usd.get_or_insert(0.0) += cost;
            }
        }
        // The answers the person marked in the room.
        for (agent_id, reaction) in Self::reactions_between(&conn, from, to)? {
            let week = weeks.entry(agent_id.clone()).or_insert_with(|| AgentWeek {
                agent_id,
                ..AgentWeek::default()
            });
            match reaction {
                Reaction::Up => week.useful += 1,
                Reaction::Down => week.not_useful += 1,
            }
        }
        let mut list: Vec<AgentWeek> = weeks
            .into_values()
            .map(|mut week| {
                week.tasks = tasks.get(&week.agent_id).map_or(0, |set| set.len() as u32);
                week
            })
            .collect();
        list.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_week_counts_its_turns_tasks_and_tokens() {
        let repo = SqliteSessionRepo::from_connection(Arc::new(Mutex::new(
            Connection::open_in_memory().unwrap(),
        )))
        .unwrap();
        let conn = repo.conn.lock().unwrap();
        conn.execute_batch(
            "INSERT INTO dcc_agents (id, name, role, avatar_color, avatar_arms, avatar_eyes, created_at, updated_at) VALUES ('qa', 'QA', 'r', 'violet', 4, 'round', 't', 't');
             INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at) VALUES ('s-task', 'p', 'w-1', 'codex', 'active', 't', 't');
             INSERT INTO dcc_sessions (id, project_id, workspace_id, provider_id, state, created_at, updated_at) VALUES ('s-home', 'dcc-agent-qa', 'w-home', 'codex', 'active', 't', 't');
             INSERT INTO dcc_session_agents (session_id, agent_id, created_at) VALUES ('s-task', 'qa', 't');
             INSERT INTO dcc_session_agents (session_id, agent_id, created_at) VALUES ('s-home', 'qa', 't');
             INSERT INTO dcc_turn_model_usage (session_id, turn_id, model, total_tokens, cached_input_tokens, cost_usd, recorded_at) VALUES ('s-task', 't-1', 'm', 1000, 0, 0.4, '2026-11-03T10:00:00.000Z');
             INSERT INTO dcc_turn_model_usage (session_id, turn_id, model, total_tokens, cached_input_tokens, cost_usd, recorded_at) VALUES ('s-home', 't-2', 'm', 500, 0, NULL, '2026-11-12T10:00:00.000Z');",
        )
        .unwrap();
        for (id, session, sequence, kind, at) in [
            (
                "e1",
                "s-task",
                1,
                "turn_completed",
                "2026-11-03T10:00:00.000Z",
            ),
            (
                "e2",
                "s-home",
                1,
                "turn_completed",
                "2026-11-04T10:00:00.000Z",
            ),
            (
                "e3",
                "s-task",
                2,
                "turn_aborted",
                "2026-11-05T10:00:00.000Z",
            ),
            (
                "e4",
                "s-task",
                3,
                "turn_completed",
                "2026-11-12T10:00:00.000Z",
            ),
        ] {
            conn.execute(
                "INSERT INTO dcc_session_events (event_id, session_id, sequence, occurred_at, kind_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, session, sequence, at, format!("{{\"type\":\"{kind}\"}}")],
            )
            .unwrap();
        }
        drop(conn);
        let week = repo
            .agents_week("2026-11-02T00:00:00.000Z", "2026-11-09T00:00:00.000Z")
            .unwrap();
        assert_eq!(week.len(), 1);
        let qa = &week[0];
        assert_eq!((qa.answers, qa.stopped, qa.tasks), (2, 1, 1));
        assert_eq!(qa.tokens, 1000);
        assert_eq!(qa.cost_usd, Some(0.4));
    }
}
