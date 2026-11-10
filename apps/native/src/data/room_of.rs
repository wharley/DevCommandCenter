//! Which room a card of the team room belongs to (`docs/PLANO_SALA_POR_PROJETO`,
//! slice 3): the project of its task, of its pull request's repository or
//! of its check-in; shown in that project's room when it has one, in the
//! Geral room otherwise, so nothing goes missing.

use std::collections::HashMap;

use super::room_inbox::{CardKind, InboxCard};
use super::tasks::{Project, TaskSummary};

/// The project of a pull request labelled `repository#number`.
pub fn label_project(label: &str, projects: &[Project]) -> Option<String> {
    let repository = label.split('#').next()?.rsplit('/').next()?.trim();
    projects
        .iter()
        .find(|project| project.name.eq_ignore_ascii_case(repository))
        .map(|project| project.project_id.clone())
}

/// The project of `card`; `pr_labels` maps a review's pull request id to
/// its `repository#number`.
pub fn card_project(
    card: &InboxCard,
    tasks: &[TaskSummary],
    projects: &[Project],
    pr_labels: &HashMap<String, String>,
) -> Option<String> {
    match &card.kind {
        CardKind::TaskResult { workspace_id, .. } | CardKind::TaskWaiting { workspace_id, .. } => {
            tasks
                .iter()
                .find(|task| task.id == *workspace_id)
                .map(|task| task.project_id.clone())
        }
        CardKind::PrReview { pr_id, .. } => label_project(pr_labels.get(pr_id)?, projects),
    }
}

/// The room of something of `project`: its room when it has one, else
/// the Geral room (none).
pub fn room_for(project: Option<String>, rooms: &[String]) -> Option<String> {
    project.filter(|project| rooms.contains(project))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: format!("r-{id}"),
            root_path: format!("/p/{name}"),
            display_name: name.to_string(),
            icon: None,
            color: None,
            pinned_at: None,
            project_id: id.to_string(),
            name: name.to_string(),
            base_branch: "main".into(),
            missing: false,
        }
    }

    fn card(kind: CardKind) -> InboxCard {
        InboxCard {
            key: "k".into(),
            agent_id: "dev".into(),
            kind,
            title: "t".into(),
            trigger: None,
            at: "2026-11-10T10:00:00Z".into(),
            unread: true,
        }
    }

    #[test]
    fn a_card_goes_to_its_projects_room_or_to_geral() {
        let projects = [
            project("checkout", "vendeagora-app"),
            project("omni", "OmniFast"),
        ];
        let mut task = TaskSummary::test("w-1");
        task.project_id = "checkout".into();
        let result = card(CardKind::TaskResult {
            workspace_id: "w-1".into(),
            session_id: "s".into(),
            completed_at: "2026-11-10T10:00:00Z".into(),
        });
        let review = card(CardKind::PrReview {
            pr_id: "pr-924".into(),
            failed: false,
            running: true,
            reason: None,
        });
        let labels: HashMap<String, String> =
            [("pr-924".to_string(), "vendeagora-app#924".to_string())].into();
        let rooms = ["checkout".to_string()];
        let task_project = card_project(&result, &[task.clone()], &projects, &labels);
        assert_eq!(room_for(task_project, &rooms).as_deref(), Some("checkout"));
        let review_project = card_project(&review, &[task], &projects, &labels);
        assert_eq!(review_project.as_deref(), Some("checkout"));
        // A project with no room: Geral.
        assert_eq!(room_for(Some("omni".into()), &rooms), None);
        assert_eq!(
            label_project("VoaHoteis/OmniFast#8", &projects).as_deref(),
            Some("omni")
        );
    }
}
