//! Drives the real shell view headlessly: press on the separator, drag, release.

use gpui::{Modifiers, MouseButton, TestAppContext, WindowAppearance, point, px};

use super::Shell;
use crate::prefs::Prefs;
use crate::screens::code_column::CodeColumnTab;
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

#[gpui::test]
fn the_sidebar_opens_clone_and_open_branch(cx: &mut TestAppContext) {
    use crate::screens::open_project::DialogMode;
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::CloneProject));
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let (dialog, _) = shell.open_project.as_ref().expect("clone opened");
        assert_eq!(dialog.read(cx).mode, DialogMode::Clone);
    });
    sidebar.update(cx, |_, cx| {
        cx.emit(SidebarEvent::OpenBranchInProject {
            project_id: "app".into(),
            root_path: "/code/app".into(),
            base_branch: "main".into(),
            label: "Checkout".into(),
        })
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let (dialog, _) = shell.open_project.as_ref().expect("open branch opened");
        let dialog = dialog.read(cx);
        assert_eq!(dialog.mode, DialogMode::Open);
        assert_eq!(
            dialog
                .context
                .as_ref()
                .map(|context| context.label.as_str()),
            Some("Checkout")
        );
    });
}

#[gpui::test]
fn cmd_shift_e_and_g_open_and_close_the_code_column(cx: &mut TestAppContext) {
    let (shell, cx, dir) = open_shell(cx, Prefs::default());
    cx.update(|_, cx| crate::screens::shell::bind_keys(cx));
    // Keys reach the views painted in the last frame.
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-shift-e");
    shell.read_with(cx, |shell, _| assert!(shell.prefs.code_column_open));
    assert!(
        Prefs::load(&dir.path().join("native-ui.json")).code_column_open,
        "saved with the layout"
    );
    cx.simulate_keystrokes("cmd-shift-e");
    shell.read_with(cx, |shell, _| assert!(!shell.prefs.code_column_open));
    // ⌘⇧G opens it on "Mudanças"; again on the same list closes it.
    cx.simulate_keystrokes("cmd-shift-g");
    shell.read_with(cx, |shell, cx| {
        assert!(shell.prefs.code_column_open);
        assert_eq!(shell.prefs.code_column_tab, CodeColumnTab::Changes);
        assert_eq!(shell.code_column.read(cx).tab, CodeColumnTab::Changes);
    });
    cx.simulate_keystrokes("cmd-shift-g");
    shell.read_with(cx, |shell, _| assert!(!shell.prefs.code_column_open));
    // The sidebar's button toggles it too.
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| {
        cx.emit(crate::screens::sidebar::SidebarEvent::ToggleCodeColumn)
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| assert!(shell.prefs.code_column_open));
}

#[gpui::test]
fn a_change_opened_from_the_column_goes_on_the_stage_and_back(cx: &mut TestAppContext) {
    use crate::screens::code_column::CodeColumnEvent;
    use crate::screens::stage::DiffRequest;
    use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let repo = crate::test_support::git_checkout(&[("a.rs", "fn a() {}\n")]);
    std::fs::write(repo.path().join("a.rs"), "fn a() { b(); }\n").unwrap();
    let request = DiffRequest {
        root: repo.path().to_string_lossy().into_owned(),
        path: "a.rs".into(),
        name: "a.rs".into(),
        status: "M".into(),
        scope: WorkspaceGitPreviewScope::Unstaged,
        base: None,
        from_changes: true,
        review: None,
    };
    let column = shell.read_with(cx, |shell, _| shell.code_column.clone());
    column.update(cx, |_, cx| cx.emit(CodeColumnEvent::OpenDiff(request)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("stage").is_some(),
        "the stage over the chat"
    );
    column.read_with(cx, |column, _| {
        assert_eq!(
            column.open_diff,
            Some((WorkspaceGitPreviewScope::Unstaged, "a.rs".to_string()))
        );
    });
    let stage = shell.read_with(cx, |shell, _| shell.stage.clone());
    stage.update(cx, |stage, cx| stage.close(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("stage").is_none());
    shell.read_with(cx, |shell, _| {
        assert!(shell.prefs.code_column_open);
        assert_eq!(shell.prefs.code_column_tab, CodeColumnTab::Changes);
    });
    column.read_with(cx, |column, _| assert_eq!(column.open_diff, None));
}

#[gpui::test]
fn a_file_from_the_tree_or_a_conversation_link_opens_on_the_stage(cx: &mut TestAppContext) {
    use crate::screens::code_column::CodeColumnEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let repo = crate::test_support::git_checkout(&[("src/a.rs", "fn a() {}\n"), ("b.rs", "b\n")]);
    let root = repo.path().to_string_lossy().into_owned();
    let column = shell.read_with(cx, |shell, _| shell.code_column.clone());
    column.update(cx, |column, cx| column.set_root(Some(root.clone()), cx));
    shell.update(cx, |shell, _| shell.stage_root = Some(root.clone()));
    column.update(cx, |_, cx| {
        cx.emit(CodeColumnEvent::OpenFile("b.rs".into()))
    });
    cx.run_until_parked();
    let stage = shell.read_with(cx, |shell, _| shell.stage.clone());
    stage.read_with(cx, |stage, _| {
        let tabs = stage.files.as_ref().expect("the file on the stage");
        assert_eq!(tabs.active().unwrap().path, "b.rs");
    });
    column.read_with(cx, |column, _| {
        assert_eq!(column.selected.as_deref(), Some("b.rs"))
    });
    // A link in a message, inside the task's checkout, at a line.
    let link = format!("{root}/src/a.rs:1");
    shell.update_in(cx, |shell, window, cx| shell.open_link(&link, window, cx));
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| {
        let tabs = stage.files.as_ref().unwrap();
        assert_eq!(tabs.active().unwrap().path, "src/a.rs");
    });
    // Outside the checkout nothing opens on the stage.
    shell.update_in(cx, |shell, window, cx| {
        shell.open_link("https://example.com", window, cx)
    });
    stage.read_with(cx, |stage, _| {
        assert_eq!(
            stage.files.as_ref().unwrap().active().unwrap().path,
            "src/a.rs"
        )
    });
}

#[gpui::test]
fn code_from_the_stage_opens_the_card_and_goes_to_the_composer(cx: &mut TestAppContext) {
    use dcc_core::domain::code_annotation::{AnnotationSide, CodeAnnotation, context_block};

    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    cx.update(|_, cx| crate::screens::annotation::bind_keys(cx));
    let annotation = CodeAnnotation {
        path: "src/lib.rs".into(),
        side: AnnotationSide::Modified,
        start_line: 4,
        end_line: 4,
        snippet: "let x = 1;".into(),
    };
    shell.update(cx, |shell, cx| {
        let composer = shell.conversation.read(cx).composer.clone();
        composer.update(cx, |composer, cx| {
            composer.set_session(
                crate::screens::composer::ComposerSession {
                    session_id: Some("a".into()),
                    ..Default::default()
                },
                cx,
            )
        });
        let request = crate::screens::annotation::AnnotationRequest {
            annotation: annotation.clone(),
            anchor: point(px(400.0), px(300.0)),
        };
        shell.stage.update(cx, |_, cx| {
            cx.emit(crate::screens::stage::StageEvent::Annotate(request))
        });
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("annotation-card").is_some(),
        "the card opens"
    );
    let card = shell.read_with(cx, |shell, _| shell.annotation.as_ref().unwrap().0.clone());
    card.update(cx, |card, cx| card.edit_in_composer(cx));
    shell.read_with(cx, |shell, cx| {
        assert!(shell.annotation.is_none(), "closed");
        let composer = shell.conversation.read(cx).composer.read(cx);
        assert_eq!(composer.input.read(cx).text(), context_block(&annotation));
    });
}
