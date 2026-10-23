use dcc_core::domain::attention::{AgentActivity, AgentActivityState};

use super::*;
use crate::data::activity::TaskActivity;
use crate::data::tasks::TaskStatus;
use crate::data::time::parse_ms;

fn task(id: &str, status: TaskStatus) -> TaskSummary {
    TaskSummary {
        id: id.into(),
        name: id.into(),
        branch: "main".into(),
        base_branch: "main".into(),
        status,
        project_id: "p".into(),
        root_path: "/repo".into(),
        worktree_path: None,
        pinned_at: None,
        snoozed_until: None,
        created_at: "2026-10-23T12:00:00Z".into(),
        updated_at: "2026-10-23T12:00:00Z".into(),
        bundle_id: None,
        member_workspace_ids: vec![],
        member_names: vec![],
        member_project_names: vec![],
        context_folders: vec![],
    }
}

fn state(tasks: Vec<TaskSummary>) -> SidebarState {
    SidebarState {
        data: SidebarData {
            tasks,
            ..SidebarData::default()
        },
        now_ms: parse_ms("2026-10-23T15:00:00Z").unwrap(),
        ..SidebarState::default()
    }
}

fn running(state: &mut SidebarState, id: &str) {
    state.data.activities.insert(
        id.into(),
        TaskActivity {
            activity: AgentActivity {
                state: AgentActivityState::Active,
                started_at: Some("2026-10-23T14:00:00Z".into()),
                completed_at: None,
                waiting_since: None,
                usage_limit: None,
            },
            last_interaction_at: None,
            waiting_session_ids: vec![],
            running_session_ids: vec![],
        },
    );
}

#[test]
fn hold_sections_show_only_with_tasks_and_rows_only_when_open() {
    let mut rail = state(vec![
        task("a", TaskStatus::Ready),
        task("paused", TaskStatus::Archived),
        task("done", TaskStatus::Completed),
    ]);
    let items = rail.items();
    assert_eq!(items[0], RailItem::DayLabel(TaskSectionId::Today));
    assert!(items.contains(&RailItem::Header {
        section: HoldSection::Waiting,
        count: 1,
        open: false
    }));
    assert!(
        !items
            .iter()
            .any(|item| matches!(item, RailItem::Row { compact: true, .. }))
    );

    rail.toggle_section(HoldSection::Completed);
    let items = rail.items();
    assert!(items.contains(&RailItem::Row {
        task_id: "done".into(),
        compact: true
    }));
    assert_eq!(items.last(), Some(&RailItem::BottomPadding));
}

#[test]
fn gaps_match_the_desktop_spacing() {
    let only_on_hold = state(vec![task("paused", TaskStatus::Archived)]);
    assert_eq!(only_on_hold.items()[0], RailItem::Gap(8.0));
    let both = state(vec![
        task("a", TaskStatus::Ready),
        task("paused", TaskStatus::Archived),
    ]);
    let items = both.items();
    assert_eq!(items[2], RailItem::Gap(10.0));
}

#[test]
fn running_jump_cycles_through_running_tasks() {
    let mut rail = state(vec![
        task("a", TaskStatus::Ready),
        task("b", TaskStatus::Ready),
        task("c", TaskStatus::Ready),
    ]);
    running(&mut rail, "a");
    running(&mut rail, "c");
    let first = rail.next_running().unwrap();
    let second = rail.next_running().unwrap();
    assert_ne!(first, second);
    assert_eq!(rail.next_running().unwrap(), first);
    assert_eq!(rail.running_ids().len(), 2);
}

#[test]
fn attention_preview_hides_the_rest() {
    let mut rail = state(
        (0..7)
            .map(|index| {
                let mut task = task(&format!("t{index}"), TaskStatus::SetupPending);
                task.worktree_path = Some("/wt".into());
                task
            })
            .collect(),
    );
    let (visible, hidden) = rail.shown_attention();
    assert_eq!((visible.len(), hidden), (5, 2));
    rail.show_all_attention = true;
    assert_eq!(rail.shown_attention(), (rail.attention(), 0));
}
