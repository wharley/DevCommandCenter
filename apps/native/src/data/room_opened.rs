//! A task an agent opened, once its own turn ends: a result card ("Dev terminou
//! “Corrigir domínio…”") the room shows in the project's room
//! (`room_of`), so a project's work is seen where the project is — the way
//! Tucano posts a project's events in its channel. The room leaves it out
//! where it was asked, whose conversation already carries the report.

use std::collections::HashMap;

use super::activity::TaskActivity;
use super::room_inbox::InboxCard;
use super::task_agents::OpenedTask;
use super::tasks::TaskSummary;

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
            // The task's own conversation, not another agent called into
            // the task (the Reviewer answering a review there has its own
            // card, in its name).
            dcc_core::domain::room_inbox::opened_task_card(
                &opened.agent_id,
                task_id,
                &task.name,
                session,
                activity.session_open(session),
                activity.session_completed_at(session),
                unread,
                now_ms,
            )
            .map(super::room_inbox::from_task_card)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::time::parse_ms;
    use dcc_core::domain::attention::{AgentActivity, AgentActivityState};

    /// The task's activity with its own conversation, `s-task`, ended at
    /// `completed` (none: still open).
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
            running_session_ids: if completed.is_none() {
                vec!["s-task".into()]
            } else {
                Vec::new()
            },
            completed_by_session: completed
                .map(|at| vec![("s-task".to_string(), at.to_string())])
                .unwrap_or_default(),
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

    #[test]
    fn another_agent_called_into_the_task_is_not_the_tasks_own_turn() {
        // The Dev opened it and its turn ended yesterday; this morning the
        // Reviewer answered a review in the same task, in its own
        // conversation there.
        let opened: HashMap<String, OpenedTask> = [(
            "w-dominio".to_string(),
            OpenedTask {
                agent_id: "dev".into(),
                session_id: "s-geral".into(),
                task_session_id: Some("s-task".into()),
            },
        )]
        .into();
        let task = TaskSummary::test("w-dominio");
        let now = parse_ms("2026-11-11T11:00:00Z").unwrap();
        let mut reviewing = activity(AgentActivityState::Active, None);
        reviewing.running_session_ids = vec!["s-revisor".into()];
        reviewing.completed_by_session = vec![("s-task".into(), "2026-11-10T19:32:00Z".into())];
        let while_reviewing: HashMap<String, TaskActivity> =
            [("w-dominio".to_string(), reviewing.clone())].into();
        let cards = opened_cards(
            &opened,
            &[task.clone()],
            &while_reviewing,
            &|_, _| true,
            now,
        );
        // The Dev's card stays the one of its own turn, yesterday's.
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].at, "2026-11-10T19:32:00Z");
        // The Reviewer done: still the Dev's turn of yesterday, no new card.
        let mut reviewed = reviewing;
        reviewed.activity.state = AgentActivityState::Completed;
        reviewed.activity.completed_at = Some("2026-11-11T10:55:00Z".into());
        reviewed.running_session_ids.clear();
        reviewed
            .completed_by_session
            .push(("s-revisor".into(), "2026-11-11T10:55:00Z".into()));
        let done: HashMap<String, TaskActivity> = [("w-dominio".to_string(), reviewed)].into();
        let cards = opened_cards(&opened, &[task], &done, &|_, _| true, now);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].at, "2026-11-10T19:32:00Z");
    }
}
