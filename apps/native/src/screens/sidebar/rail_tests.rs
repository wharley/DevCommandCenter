//! The collapsed rail: each task shows its project's face, and a dot when it
//! needs the person.

use gpui::{TestAppContext, WindowAppearance};

use super::Sidebar;
use crate::data::loader::SidebarData;
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn the_rail_marks_only_the_tasks_that_need_the_person(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let (sidebar, cx) = cx.add_window_view(|_, cx| Sidebar::new(true, false, false, seen, cx));
    // A worktree still being prepared waits on the person; a ready one does not.
    let mut waiting = TaskSummary::test("w-setup");
    waiting.status = TaskStatus::SetupPending;
    waiting.worktree_path = Some("/wt".into());
    let data = SidebarData {
        tasks: vec![waiting, TaskSummary::test("w-ready")],
        ..SidebarData::default()
    };
    sidebar.update(cx, |sidebar, cx| sidebar.apply_for_tests(data, cx));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();

    let tile = cx.debug_bounds("rail-task-dot-w-setup").expect("a dot");
    assert!(tile.size.width > gpui::px(0.0));
    assert!(cx.debug_bounds("rail-task-dot-w-ready").is_none());
}

/// The rail drawn by Metal, saved to look at:
/// `DCC_RAIL_PICTURE=/tmp/x.png cargo test rail_picture -- --ignored`.
#[test]
#[ignore = "renders with the GPU"]
fn rail_picture() {
    use std::sync::Arc;

    use gpui::{AppContext, px, size};

    use crate::data::tasks::Project;

    let out = std::env::var("DCC_RAIL_PICTURE").expect("DCC_RAIL_PICTURE");
    let mut cx = gpui::HeadlessAppContext::with_platform(
        Arc::new(gpui_apple::AppleTextSystem::new()),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Dark, WindowAppearance::Dark));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let project = |root: &str| Project {
        id: root.into(),
        root_path: root.into(),
        display_name: root.into(),
        icon: None,
        color: None,
        pinned_at: None,
        project_id: root.into(),
        name: root.into(),
        base_branch: "main".into(),
        missing: false,
    };
    let task = |id: &str, root: &str| {
        let mut task = TaskSummary::test(id);
        task.root_path = root.into();
        task
    };
    let mut waiting = task("w-setup", "/omnifast");
    waiting.status = TaskStatus::SetupPending;
    waiting.worktree_path = Some("/wt".into());
    let data = SidebarData {
        projects: vec![project("/omnifast"), project("/duo"), project("/dcc")],
        tasks: vec![
            task("w-one", "/omnifast"),
            waiting,
            task("w-duo", "/duo"),
            task("w-dcc", "/dcc"),
            task("w-loose", "/elsewhere"),
        ],
        ..SidebarData::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let window = cx
        .open_window(size(px(76.0), px(760.0)), |_, cx| {
            cx.new(|cx| Sidebar::new(true, false, false, seen, cx))
        })
        .unwrap();
    // After the first read, which finds no database here.
    cx.run_until_parked();
    cx.update(|cx| {
        window.update(cx, |sidebar, _, cx| {
            sidebar.apply_for_tests(data, cx);
            sidebar.state.selected_task_id = Some("w-one".into());
        })
    })
    .unwrap();
    cx.update_window(window.into(), |_, window, _| window.refresh())
        .unwrap();
    cx.run_until_parked();
    let image = cx.capture_screenshot(window.into()).unwrap();
    image.save(&out).unwrap();
}
