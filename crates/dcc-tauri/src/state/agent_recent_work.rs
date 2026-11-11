//! What an agent did in tasks since its conversation's last turn
//! (`dcc_core::domain::recent_work`): its conversations in tasks — of the
//! room's project in a project's room, any in Geral — whose last turn ended
//! after that, with what started the turn and its final answer. Each task
//! conversation is read only when its last turn is new.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use dcc_core::domain::agent_conversation::home_project_id;
use dcc_core::domain::recent_work::{recent_work_block, TaskWork};
use dcc_core::domain::team_room_lines::{lines_of, Speaker};
use dcc_core::domain::timeline::project_session;

use super::*;

/// With no turn before (a new conversation), work this recent counts.
const FIRST_TURN_WINDOW_HOURS: i64 = 24;
/// Never further back than this.
const MAX_WINDOW_DAYS: i64 = 7;

fn parse(at: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

impl SessionCommandState {
    /// The block for `session_id`'s next turn, the conversation of
    /// `agent_id`; none when nothing new happened in its tasks.
    pub(super) fn recent_work(
        &self,
        session_id: &SessionId,
        agent_id: &str,
    ) -> Result<Option<String>> {
        let now = Utc::now();
        let since = self
            .session_repo
            .last_turn_end(session_id)?
            .as_deref()
            .and_then(parse)
            .unwrap_or(now - Duration::hours(FIRST_TURN_WINDOW_HOURS))
            .max(now - Duration::days(MAX_WINDOW_DAYS));
        let room = self.session_room(session_id)?;
        let home = home_project_id(agent_id);
        let mut work = Vec::new();
        for binding in self.list_agent_session_bindings()? {
            if binding.agent_id != agent_id
                || binding.project_id == home
                || room
                    .as_ref()
                    .is_some_and(|project| *project != binding.project_id)
            {
                continue;
            }
            let id = SessionId(binding.session_id.clone());
            let Some(ended) = self.session_repo.last_turn_end(&id)? else {
                continue;
            };
            if parse(&ended).is_none_or(|ended| ended <= since) {
                continue;
            }
            let events = self.session_repo.list_events_by_session_sync(&id)?;
            let messages = project_session(&events, &binding.session_id);
            let lines = lines_of(agent_id, &binding.session_id, &messages, &HashMap::new());
            let Some(answer_at) = lines
                .iter()
                .rposition(|line| line.speaker == Speaker::Agent)
            else {
                continue;
            };
            let asked = lines[..answer_at]
                .iter()
                .rev()
                .find(|line| line.speaker == Speaker::Person)
                .map(|line| line.text.clone())
                .unwrap_or_default();
            work.push(TaskWork {
                task: self
                    .session_repo
                    .workspace_name(&binding.workspace_id)
                    .or(binding.title.clone())
                    .unwrap_or_else(|| "a task".to_string()),
                asked,
                answer: lines[answer_at].text.clone(),
                at: ended,
            });
        }
        Ok(recent_work_block(work))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::agent::{ResidentAgentDraft, ReviewerPresetText};
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::session::{Session, SessionEventKind, SessionEventRecord, SessionState};
    use dcc_core::domain::workspace::WorkspaceId;
    use dcc_core::ports::{SessionEventRepo, SessionRepo};
    use serde_json::json;

    fn session(id: &str, project: &str) -> Session {
        Session {
            id: SessionId(id.to_string()),
            project_id: ProjectId(project.to_string()),
            workspace_id: WorkspaceId(format!("w-{id}")),
            additional_workspace_ids: Vec::new(),
            provider_id: "claude_code".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-11-10T10:00:00Z".to_string(),
            updated_at: "2026-11-10T10:00:00Z".to_string(),
        }
    }

    async fn turn(
        state: &SessionCommandState,
        session: &str,
        sequence: u64,
        prompt: &str,
        answer: &str,
        at: DateTime<Utc>,
    ) {
        let kinds = [
            json!({ "type": "turn_started", "turnId": format!("t{sequence}"), "prompt": prompt }),
            json!({ "type": "turn_delta", "turnId": format!("t{sequence}"), "content": answer }),
            json!({ "type": "turn_completed", "turnId": format!("t{sequence}") }),
        ];
        for (offset, kind) in kinds.into_iter().enumerate() {
            let kind: SessionEventKind = serde_json::from_value(kind).unwrap();
            let event = SessionEventRecord {
                event_id: format!("{session}-{sequence}-{offset}"),
                session_id: SessionId(session.to_string()),
                sequence: sequence + offset as u64,
                occurred_at: (at + Duration::seconds(offset as i64)).to_rfc3339(),
                kind,
            };
            SessionEventRepo::append_event(state, &event).await.unwrap();
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_rooms_conversation_hears_what_its_agent_did_in_the_projects_tasks() {
        let root = tempfile::tempdir().unwrap();
        let physical = std::fs::canonicalize(root.path()).unwrap();
        let state = SessionCommandState::new_headless(
            physical.join("state.sqlite"),
            physical.join("app-data"),
        );
        let mut draft = ResidentAgentDraft::reviewer(&ReviewerPresetText::default());
        draft.name = "Revisor".into();
        let agent = state.save_resident_agent(None, draft).unwrap();
        let room = session("room", &home_project_id(&agent.id));
        let review = session("review", "checkout");
        let elsewhere = session("other", "site");
        for each in [&room, &review, &elsewhere] {
            SessionRepo::save_session(&state, each).await.unwrap();
            state.bind_session_agent(&each.id, &agent.id).unwrap();
        }
        state.set_session_room(&room.id, "checkout").unwrap();
        let now = Utc::now();
        // The room's last turn, then a trigger's review in a Checkout task
        // and another in a task of another project.
        turn(&state, "room", 1, "oi", "Olá.", now - Duration::hours(3)).await;
        turn(
            &state,
            "review",
            1,
            "[DCC] New review activity on this task's pull request acme/app#1049 from @keppelen.\n\nRead what they asked",
            "O comentário do @keppelen procede.",
            now - Duration::hours(1),
        )
        .await;
        turn(
            &state,
            "other",
            1,
            "veja",
            "Outro projeto.",
            now - Duration::hours(1),
        )
        .await;
        let block = state.recent_work(&room.id, &agent.id).unwrap().unwrap();
        assert!(block.contains("O comentário do @keppelen procede."));
        assert!(block.contains("acme/app#1049 from @keppelen."));
        assert!(!block.contains("Outro projeto."));
        // Once the room's conversation answered after it, it is not new.
        turn(&state, "room", 4, "e aí?", "Tudo certo.", now).await;
        assert!(state.recent_work(&room.id, &agent.id).unwrap().is_none());
    }
}
