//! Drives the real shell view headlessly: press on the separator, drag, release.

use gpui::{Modifiers, MouseButton, TestAppContext, WindowAppearance, point, px};

use super::Shell;
use crate::prefs::Prefs;
use crate::theme::{Theme, ThemePreference};

fn open_shell(
    cx: &mut TestAppContext,
    prefs: Prefs,
) -> (
    gpui::Entity<Shell>,
    &mut gpui::VisualTestContext,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native-ui.json");
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (shell, cx) = cx.add_window_view(|window, cx| Shell::new(prefs, path, window, cx));
    (shell, cx, dir)
}

#[gpui::test]
fn dragging_the_separator_resizes_and_saves(cx: &mut TestAppContext) {
    let (shell, cx, dir) = open_shell(cx, Prefs::default());
    let y = px(300.0);

    cx.simulate_mouse_down(point(px(336.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(380.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(436.0), y), MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| {
        assert!(shell.layout.is_resizing());
        assert_eq!(shell.layout.sidebar_width, 436.0);
    });

    cx.simulate_mouse_up(point(px(436.0), y), MouseButton::Left, Modifiers::none());
    shell.read_with(cx, |shell, _| assert!(!shell.layout.is_resizing()));

    let saved = Prefs::load(&dir.path().join("native-ui.json"));
    assert_eq!(saved.sidebar_width, 436.0);
}

#[gpui::test]
fn dragging_past_the_limit_stops_at_the_max(cx: &mut TestAppContext) {
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let y = px(300.0);

    cx.simulate_mouse_down(point(px(336.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(1200.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point(px(1200.0), y), MouseButton::Left, Modifiers::none());

    shell.read_with(cx, |shell, _| assert_eq!(shell.layout.sidebar_width, 520.0));
}

#[gpui::test]
fn pressing_away_from_the_separator_does_nothing(cx: &mut TestAppContext) {
    let (shell, cx, dir) = open_shell(cx, Prefs::default());
    let y = px(300.0);

    cx.simulate_mouse_down(point(px(700.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(800.0), y), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(point(px(800.0), y), MouseButton::Left, Modifiers::none());

    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.layout.sidebar_width, 336.0);
        assert!(!shell.layout.is_resizing());
    });
    assert!(
        !dir.path().join("native-ui.json").exists(),
        "nothing to save"
    );
}

#[gpui::test]
fn a_collapsed_sidebar_opens_as_the_rail(cx: &mut TestAppContext) {
    let prefs = Prefs {
        sidebar_collapsed: true,
        ..Prefs::default()
    };
    let (shell, cx, _dir) = open_shell(cx, prefs);
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.layout.sidebar_rail_width(), 76.0);
        assert_eq!(shell.layout.sidebar_width, 336.0);
    });
}

#[gpui::test]
fn new_task_shows_its_screen_and_a_task_brings_the_conversation_back(cx: &mut TestAppContext) {
    use super::Surface;
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::NewTask));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| assert_eq!(shell.surface, Surface::NewTask));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("new-task").is_some());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenTask("any".into())));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.surface, Surface::Conversation)
    });
}

#[gpui::test]
fn several_projects_opens_the_open_project_dialog(cx: &mut TestAppContext) {
    use crate::screens::new_task::NewTaskEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let new_task = shell.read_with(cx, |shell, _| shell.new_task.clone());
    new_task.update(cx, |_, cx| {
        cx.emit(NewTaskEvent::OpenProject {
            multi: true,
            worktree: false,
        })
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let (dialog, _) = shell.open_project.as_ref().expect("the dialog opened");
        let dialog = dialog.read(cx);
        assert!(dialog.multi);
        assert!(!dialog.model.worktree, "the screen's choice comes along");
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("open-project").is_some());
}
