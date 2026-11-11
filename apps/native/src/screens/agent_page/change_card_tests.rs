//! The report's change card, driven headlessly: a task's files read once
//! as the card shows (a fake reader, never git), three then all; its
//! delivery footer, whose step only opens the task's own dialog; and no
//! actions for a task a turn runs in or one closed.

use std::cell::Cell;

use dcc_core::domain::attention::{AgentActivity, AgentActivityState};
use dcc_core::domain::workspace_git::{
    WorkspaceGitChangeEntry, WorkspaceGitStatusOutput, WorkspacePrStatusOutput,
};
use gpui::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{AgentPage, AgentPageEvent};
use crate::data::activity::TaskActivity;
use crate::data::git_watch::GitSnapshot;
use crate::data::loader::SidebarData;
use crate::data::task_changes::{ChangedFile, ChangesRead, ChangesRequest};
use crate::data::task_delivery::DeliveryStep;
use crate::data::task_reports::{ReportState, TaskReport};
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::data::team_room::{HomeFeed, RoomLine, Speaker};
use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;
use crate::theme::{Theme, ThemePreference};

thread_local! {
    /// Reads made on this test's thread (the test executor runs the
    /// background work there too).
    static READS: Cell<usize> = const { Cell::new(0) };
}

fn reads() -> usize {
    READS.with(Cell::get)
}

/// Five files committed on the branch.
fn fake_read(request: &ChangesRequest) -> ChangesRead {
    READS.with(|reads| reads.set(reads.get() + 1));
    assert_eq!(request.root.as_deref(), Some("/w/campo"));
    ChangesRead {
        committed: (0..5)
            .map(|n| ChangedFile {
                path: format!("src/checkout/f{n}.rs"),
                additions: 10 - n,
                deletions: 1,
            })
            .collect(),
        uncommitted: Vec::new(),
        from_checkout: true,
    }
}

fn status() -> WorkspaceGitStatusOutput {
    WorkspaceGitStatusOutput {
        staged: vec![],
        unstaged: vec![WorkspaceGitChangeEntry {
            path: "README.md".into(),
            name: "README.md".into(),
            absolute_path: "/w/campo/README.md".into(),
            status: "M".into(),
            insertions: 2,
            deletions: 0,
        }],
        staged_fingerprint: String::new(),
        current_branch: Some("dcc/campo".into()),
        ahead_of_remote_count: 0,
        behind_of_remote_count: 0,
        conflict_count: 0,
        merge_in_progress: false,
        has_remote: true,
    }
}

fn open_pr() -> WorkspacePrStatusOutput {
    WorkspacePrStatusOutput {
        checks_failing: None,
        provider: Some("github".into()),
        host: None,
        number: Some(1032),
        title: None,
        url: Some("https://github.com/acme/app/pull/1032".into()),
        head_branch: Some("dcc/campo".into()),
        base_branch: Some("main".into()),
        state: Some("OPEN".into()),
        is_draft: false,
        mergeable: Some("MERGEABLE".into()),
        merge_state_status: Some("CLEAN".into()),
    }
}

fn task(status: TaskStatus) -> TaskSummary {
    TaskSummary {
        worktree_path: Some("/w/campo".into()),
        branch: "dcc/campo".into(),
        status,
        ..TaskSummary::test("w-campo")
    }
}

fn running() -> TaskActivity {
    TaskActivity {
        activity: AgentActivity {
            state: AgentActivityState::Active,
            started_at: Some("2026-11-10T15:39:00Z".into()),
            completed_at: None,
            waiting_since: None,
            usage_limit: None,
        },
        last_interaction_at: None,
        waiting_session_ids: Vec::new(),
        running_session_ids: vec!["child".into()],
        completed_by_session: Vec::new(),
    }
}

/// The room with one answer reporting the task, its events collected.
fn room_with_task(
    cx: &mut TestAppContext,
    data: SidebarData,
    snapshot: Option<GitSnapshot>,
) -> (
    Entity<AgentPage>,
    &mut VisualTestContext,
    std::rc::Rc<std::cell::RefCell<Vec<AgentPageEvent>>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (page, cx) = cx.add_window_view(move |window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
        let composer = cx.new(|cx| Composer::new(window, cx));
        AgentPage::new("dev".into(), sidebar, composer, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(1200.0), gpui::px(1600.0)));
    let mut dev = super::editor_role::blank_agent();
    dev.id = "dev".into();
    dev.name = "Dev".into();
    let report = TaskReport {
        title: "Campo X na Shopify".into(),
        state: ReportState::TurnEnded,
        task_id: Some("w-campo".into()),
        provider_id: "claude_code".into(),
        agent_id: Some("dev".into()),
        files: 6,
        additions: 42,
        deletions: 5,
        seconds: Some(360),
        session_id: Some("child".into()),
    };
    page.update(cx, |page, cx| {
        page.sidebar.update(cx, |sidebar, cx| {
            sidebar.apply_for_tests(
                SidebarData {
                    agents: vec![dev],
                    task_reports: [("t-1".to_string(), report)].into(),
                    ..data
                },
                cx,
            );
            if let Some(snapshot) = snapshot {
                sidebar.set_git_for_tests("w-campo", snapshot, cx);
            }
        });
        page.open_room(None, cx);
    });
    cx.run_until_parked();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = events.clone();
    page.update(cx, |page, cx| {
        cx.subscribe_self(move |_, event: &AgentPageEvent, _| {
            sink.borrow_mut().push(match event {
                AgentPageEvent::OpenTaskChanges(task) => {
                    AgentPageEvent::OpenTaskChanges(task.clone())
                }
                AgentPageEvent::DeliverTask { task_id, step } => AgentPageEvent::DeliverTask {
                    task_id: task_id.clone(),
                    step: *step,
                },
                AgentPageEvent::OpenPullRequest(id) => AgentPageEvent::OpenPullRequest(id.clone()),
                AgentPageEvent::OpenTask(task) => AgentPageEvent::OpenTask(task.clone()),
                AgentPageEvent::OpenSession {
                    task_id,
                    session_id,
                } => AgentPageEvent::OpenSession {
                    task_id: task_id.clone(),
                    session_id: session_id.clone(),
                },
            });
        })
        .detach();
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, cx| {
            room.changes.borrow_mut().reader = fake_read;
            let mut feed = HomeFeed::new("dev", "s-home");
            feed.lines = vec![RoomLine {
                key: "s-home:t".into(),
                agent_id: "dev".into(),
                session_id: "s-home".into(),
                speaker: Speaker::Agent,
                text: "Implementei o envio do campo.".into(),
                at: "2026-11-09T10:06:00Z".into(),
                receipt: None,
                tasks: vec!["t-1".into()],
                steps: Default::default(),
            }];
            room.take(vec![feed]);
            cx.notify();
        });
    });
    cx.run_until_parked();
    (page, cx, events)
}

fn snapshot(pr: Option<WorkspacePrStatusOutput>) -> GitSnapshot {
    GitSnapshot {
        git: Some(status()),
        pr,
        git_at: Some(1),
        pr_at: Some(1),
        ..GitSnapshot::default()
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn a_tasks_files_show_three_then_all_read_once(cx: &mut TestAppContext) {
    let data = SidebarData {
        tasks: vec![task(TaskStatus::Ready)],
        ..SidebarData::default()
    };
    let (page, cx, events) = room_with_task(cx, data, Some(snapshot(None)));
    assert!(
        cx.debug_bounds("room-report-0-0").is_some(),
        "the task's row stays"
    );
    // The five committed and the one the rail sees uncommitted: three show.
    for row in [
        "room-report-file-0-0-0",
        "room-report-file-0-0-1",
        "room-report-file-0-0-2",
    ] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(cx.debug_bounds("room-report-file-0-0-3").is_none());
    assert!(cx.debug_bounds("room-report-more-0-0").is_some());
    // Frames and notifications after it read nothing more.
    let room = page.read_with(cx, |page, _| page.room.as_ref().unwrap().0.clone());
    for _ in 0..3 {
        room.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert_eq!(reads(), 1);

    click(cx, "room-report-more-0-0");
    assert!(
        cx.debug_bounds("room-report-file-0-0-5").is_some(),
        "all six"
    );
    assert!(cx.debug_bounds("room-report-file-0-0-6").is_none());
    assert!(
        cx.debug_bounds("room-report-more-0-0").is_some(),
        "Mostrar menos"
    );
    click(cx, "room-report-more-0-0");
    assert!(cx.debug_bounds("room-report-file-0-0-3").is_none());
    assert_eq!(reads(), 1, "folding reads nothing");

    // "Ver mudanças" opens the task on its changes.
    click(cx, "room-report-changes-0-0");
    assert!(matches!(
        events.borrow().last(),
        Some(AgentPageEvent::OpenTaskChanges(task)) if task == "w-campo"
    ));
}

#[gpui::test]
fn the_step_only_opens_the_tasks_own_dialog(cx: &mut TestAppContext) {
    let data = SidebarData {
        tasks: vec![task(TaskStatus::Ready)],
        ..SidebarData::default()
    };
    let (page, cx, events) = room_with_task(cx, data, Some(snapshot(Some(open_pr()))));
    // A desktop that would hear any call the click made.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    page.update(cx, |page, cx| {
        let room = page.room.as_ref().unwrap().0.clone();
        room.update(cx, |room, _| {
            room.api = Some(crate::data::desktop_api::DesktopApi::new(port, "k"));
        });
    });
    assert!(cx.debug_bounds("room-report-delivery-0-0").is_some());
    // New work on an open pull request: commit and push, and the link.
    click(cx, "room-report-deliver-0-0");
    assert!(matches!(
        events.borrow().last(),
        Some(AgentPageEvent::DeliverTask { task_id, step: DeliveryStep::Commit { push: true } })
            if task_id == "w-campo"
    ));
    click(cx, "room-report-request-0-0");
    assert!(matches!(
        events.borrow().last(),
        Some(AgentPageEvent::OpenPullRequest(url)) if url == "https://github.com/acme/app/pull/1032"
    ));
    // Nothing reached the desktop: the dialog runs the step, in the task.
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err(), "no call from the room");
    // No merge here, even with the pull request ready.
    assert!(cx.debug_bounds("room-report-merge-0-0").is_none());
}

#[gpui::test]
fn a_task_at_work_or_closed_shows_its_state_only(cx: &mut TestAppContext) {
    let data = SidebarData {
        tasks: vec![task(TaskStatus::Ready)],
        activities: [("w-campo".to_string(), running())].into(),
        ..SidebarData::default()
    };
    let (_page, cx, _) = room_with_task(cx, data, Some(snapshot(None)));
    assert!(
        cx.debug_bounds("room-report-delivery-0-0").is_some(),
        "its state"
    );
    assert!(cx.debug_bounds("room-report-deliver-0-0").is_none());
    assert!(cx.debug_bounds("room-report-changes-0-0").is_none());
    assert!(cx.debug_bounds("room-report-request-0-0").is_none());
}

#[gpui::test]
fn a_completed_task_shows_its_state_only(cx: &mut TestAppContext) {
    let data = SidebarData {
        tasks: vec![task(TaskStatus::Completed)],
        ..SidebarData::default()
    };
    let (_page, cx, _) = room_with_task(cx, data, None);
    assert!(cx.debug_bounds("room-report-delivery-0-0").is_some());
    assert!(cx.debug_bounds("room-report-deliver-0-0").is_none());
    assert!(cx.debug_bounds("room-report-changes-0-0").is_none());
}
