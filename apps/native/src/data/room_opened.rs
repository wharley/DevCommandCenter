//! A task an agent opened, once its turn ends: a result card ("Dev terminou
//! “Corrigir domínio…”") the room shows in the project's room
//! (`room_of`), so a project's work is seen where the project is — the way
//! Tucano posts a project's events in its channel. The room leaves it out
//! where it was asked, whose conversation already carries the report.

use std::collections::HashMap;

use super::activity::TaskActivity;
use super::room_inbox::{CardKind, InboxCard};
use super::task_agents::OpenedTask;
use super::tasks::TaskSummary;
use super::time::parse_ms;

/// Kept this long, as the room's other cards.
const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

pub fn opened_cards(
    opened: &HashMap<String, OpenedTask>,
    tasks: &[TaskSummary],
    activities: &HashMap<String, TaskActivity>,
    unread: &dyn Fn(&str, &str) -> bool,
    now_ms: i64,
) -> Vec<InboxCard> {
    opened
        .iter()
        .filter_map(|(task_id, opened)| {
            let session = opened.task_session_id.as_deref()?;
            let task = tasks.iter().find(|task| task.id == *task_id)?;
            let activity = activities.get(task_id)?;
            if activity.is_turn_open() {
                return None;
            }
            let completed = activity.activity.completed_at.as_deref()?;
            if parse_ms(completed).is_none_or(|at| now_ms - at > KEEP_MS) {
                return None;
            }
            Some(InboxCard {
                key: format!("done:{session}:{completed}"),
                agent_id: opened.agent_id.clone(),
                kind: CardKind::TaskResult {
                    workspace_id: task_id.clone(),
                    session_id: session.to_string(),
                    completed_at: completed.to_string(),
                },
                title: task.name.clone(),
                trigger: None,
                at: completed.to_string(),
                unread: unread(session, completed),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::attention::{AgentActivity, AgentActivityState};

    fn activity(state: AgentActivityState, completed: Option<&str>) -> TaskActivity {
        TaskActivity {
            activity: AgentActivity {
                state,
                started_at: Some("2026-11-10T18:39:00Z".into()),
                completed_at: completed.map(str::to_string),
                waiting_since: None,
                usage_limit: None,
            },
            last_interaction_at: None,
            waiting_session_ids: Vec::new(),
            running_session_ids: Vec::new(),
        }
    }

    #[test]
    fn a_finished_task_an_agent_opened_is_a_result_card() {
        let opened: HashMap<String, OpenedTask> = [(
            "w-dominio".to_string(),
            OpenedTask {
                agent_id: "dev".into(),
                session_id: "s-geral".into(),
                task_session_id: Some("s-task".into()),
            },
        )]
        .into();
        let mut task = TaskSummary::test("w-dominio");
        task.name = "Corrigir domínio".into();
        let now = parse_ms("2026-11-10T19:00:00Z").unwrap();
        let done: HashMap<String, TaskActivity> = [(
            "w-dominio".to_string(),
            activity(AgentActivityState::Completed, Some("2026-11-10T18:45:00Z")),
        )]
        .into();
        let cards = opened_cards(&opened, &[task.clone()], &done, &|_, _| true, now);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].agent_id, "dev");
        assert_eq!(cards[0].title, "Corrigir domínio");
        // Still running: no card yet.
        let running: HashMap<String, TaskActivity> = [(
            "w-dominio".to_string(),
            activity(AgentActivityState::Active, None),
        )]
        .into();
        assert!(opened_cards(&opened, &[task], &running, &|_, _| true, now).is_empty());
    }
}
