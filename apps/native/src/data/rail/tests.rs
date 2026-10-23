use std::collections::HashMap;

use dcc_core::domain::attention::{AgentActivity, AgentActivityState, AttentionReason};

use super::*;
use crate::data::activity::TaskActivity;
use crate::data::tasks::{TaskStatus, TaskSummary};

const NOW: &str = "2026-10-23T15:00:00Z";

fn now() -> i64 {
    parse_ms(NOW).unwrap()
}

fn task(id: &str, created_at: &str) -> TaskSummary {
    TaskSummary {
        id: id.into(),
        name: id.into(),
        branch: "main".into(),
        base_branch: "main".into(),
        status: TaskStatus::Ready,
        project_id: "p".into(),
        root_path: "/repo".into(),
        worktree_path: None,
        pinned_at: None,
        snoozed_until: None,
        created_at: created_at.into(),
        updated_at: created_at.into(),
        bundle_id: None,
        member_workspace_ids: vec![],
        member_names: vec![],
        member_project_names: vec![],
        context_folders: vec![],
    }
}

fn activity(state: AgentActivityState, completed_at: Option<&str>) -> TaskActivity {
    TaskActivity {
        activity: AgentActivity {
            state,
            started_at: Some("2026-10-23T14:00:00Z".into()),
            completed_at: completed_at.map(str::to_string),
            waiting_since: None,
            usage_limit: None,
        },
        last_interaction_at: None,
        waiting_session_ids: vec![],
        running_session_ids: vec![],
    }
}

fn ids(section: &TaskSection) -> Vec<&str> {
    section.tasks.iter().map(|task| task.id.as_str()).collect()
}

#[test]
fn split_puts_paused_and_snoozed_on_hold() {
    let mut archived = task("archived", NOW);
    archived.status = TaskStatus::Archived;
    let mut done = task("done", NOW);
    done.status = TaskStatus::Completed;
    let mut snoozed = task("snoozed", NOW);
    snoozed.snoozed_until = Some("2026-10-24T09:00:00Z".into());
    let mut woke = task("woke", NOW);
    woke.snoozed_until = Some("2026-10-23T09:00:00Z".into());
    let all = vec![task("a", NOW), archived, done, snoozed, woke];
    let split = split(&all, now());
    let names = |list: &[TaskSummary]| list.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
    assert_eq!(names(&split.active), ["a", "woke"]);
    assert_eq!(names(&split.waiting), ["archived", "snoozed"]);
    assert_eq!(names(&split.completed), ["done"]);
}

#[test]
fn tasks_are_grouped_by_day_newest_first_with_pins_on_top() {
    let mut pinned = task("pinned", "2026-01-01T00:00:00Z");
    pinned.pinned_at = Some("2026-10-01T00:00:00Z".into());
    let list = vec![
        task("old", "2026-09-01T12:00:00Z"),
        task("today-early", "2026-10-23T13:00:00Z"),
        task("today-late", "2026-10-23T14:30:00Z"),
        pinned,
    ];
    let sections = task_list(&list, &HashMap::new(), now(), None);
    assert_eq!(sections[0].id, TaskSectionId::Pinned);
    assert_eq!(ids(&sections[0]), ["pinned"]);
    assert_eq!(sections[1].id, TaskSectionId::Today);
    assert_eq!(ids(&sections[1]), ["today-late", "today-early"]);
    assert_eq!(sections.last().unwrap().id, TaskSectionId::Older);
}

#[test]
fn a_new_turn_moves_a_task_up_unless_the_list_is_frozen() {
    let list = vec![
        task("a", "2026-10-23T13:00:00Z"),
        task("b", "2026-10-23T14:00:00Z"),
    ];
    let before = task_list(&list, &HashMap::new(), now(), None);
    assert_eq!(ids(&before[0]), ["b", "a"]);

    let mut activities = HashMap::new();
    let mut fresh = activity(AgentActivityState::Active, None);
    fresh.last_interaction_at = Some("2026-10-23T14:50:00Z".into());
    activities.insert("a".to_string(), fresh);

    let live = task_list(&list, &activities, now(), None);
    assert_eq!(ids(&live[0]), ["a", "b"]);

    let frozen = freeze(&before);
    let held = task_list(&list, &activities, now(), Some(&frozen));
    assert_eq!(ids(&held[0]), ["b", "a"], "nothing moves under the pointer");

    let with_new = [list.clone(), vec![task("c", "2026-10-23T12:00:00Z")]].concat();
    let held = task_list(&with_new, &activities, now(), Some(&frozen));
    assert_eq!(ids(&held[0]), ["c", "b", "a"], "new tasks go on top");
}

#[test]
fn running_jump_walks_forward_and_wraps() {
    let order = ["a", "b", "c", "d"];
    let running = |id: &str| id == "b" || id == "d";
    assert_eq!(next_running_task_id(&order, &running, None), Some("b"));
    assert_eq!(next_running_task_id(&order, &running, Some("b")), Some("d"));
    assert_eq!(next_running_task_id(&order, &running, Some("d")), Some("b"));
    assert_eq!(next_running_task_id(&order, &|_| false, None), None);
}

#[test]
fn needs_you_drops_results_already_seen() {
    let list = vec![
        task("asks", NOW),
        task("done-new", NOW),
        task("done-seen", NOW),
    ];
    let mut activities = HashMap::new();
    activities.insert(
        "asks".to_string(),
        activity(
            AgentActivityState::Waiting(AttentionReason::Permission),
            None,
        ),
    );
    activities.insert(
        "done-new".to_string(),
        activity(AgentActivityState::Completed, Some("2026-10-23T14:40:00Z")),
    );
    activities.insert(
        "done-seen".to_string(),
        activity(AgentActivityState::Completed, Some("2026-10-23T14:45:00Z")),
    );
    let unread = |id: &str, _: Option<&str>| id != "done-seen";
    let items = needs_you(&list, &activities, &[], &unread, now());
    let reasons: Vec<(&str, AttentionReason)> = items
        .iter()
        .map(|item| (item.workspace_id.as_str(), item.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            ("done-new", AttentionReason::Completed),
            ("asks", AttentionReason::Permission)
        ]
    );
}
