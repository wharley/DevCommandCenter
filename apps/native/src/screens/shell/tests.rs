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

#[gpui::test]
fn a_delegation_file_chip_reviews_it_and_opens_the_files_diff(cx: &mut TestAppContext) {
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let worktree = crate::test_support::git_checkout(&[("main.rs", "fn main() {}\n")]);
    std::fs::write(worktree.path().join("main.rs"), "fn main() { run(); }\n").unwrap();
    let root = worktree.path().to_string_lossy().into_owned();
    shell.update(cx, |shell, cx| {
        let reviewable = crate::screens::code_column::Reviewable {
            id: "d-1".into(),
            provider_id: "codex".into(),
            parent_session_id: "s-parent".into(),
            child_session_id: Some("s-child".into()),
            touched_files: 1,
            worktree: root.clone(),
        };
        shell.code_column.update(cx, |column, cx| {
            column.set_root(Some(root.clone()), cx);
            column.set_reviews(Some("s-parent".into()), vec![reviewable], cx)
        });
        shell.conversation.update(cx, |_, cx| {
            cx.emit(
                crate::screens::conversation::ConversationEvent::ReviewDelegation {
                    id: "d-1".into(),
                    path: Some("main.rs".into()),
                },
            )
        });
    });
    cx.run_until_parked();
    shell.read_with(cx, |shell, cx| {
        let stage = shell.stage.read(cx);
        let diff = stage.diff.as_ref().expect("the file's diff on the stage");
        assert_eq!(diff.request.root, root);
        assert_eq!(diff.request.path, "main.rs");
        assert!(
            shell.code_column.read(cx).delegation_review.is_some(),
            "under review"
        );
    });
}

#[gpui::test]
fn the_merge_resolver_takes_the_conversations_place_until_closed(cx: &mut TestAppContext) {
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    shell.update(cx, |shell, cx| {
        let request = crate::screens::merge::MergeResolverRequest {
            root: "/tmp/checkout".into(),
            task_id: Some("w-1".into()),
            base_branch: None,
            forge_login: None,
            workspace_label: None,
            agent: None,
        };
        shell.code_column.update(cx, |_, cx| {
            cx.emit(crate::screens::code_column::CodeColumnEvent::OpenMergeResolver(request))
        });
    });
    cx.run_until_parked();
    let resolver = shell.read_with(cx, |shell, cx| {
        assert!(!shell.code_column_shown(cx), "the column folds away");
        shell.merge_resolver().expect("open")
    });
    resolver.update(cx, |_, cx| {
        cx.emit(crate::screens::merge::MergeResolverEvent::Closed)
    });
    shell.read_with(cx, |shell, _| assert!(shell.merge_resolver().is_none()));
}

#[gpui::test]
fn cmd_f_outside_a_field_opens_the_find_bar(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    cx.update(|cx| {
        super::bind_keys(cx);
        crate::screens::conversation::bind_keys(cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenTask("any".into())));
    cx.run_until_parked();
    let focus = shell.read_with(cx, |shell, _| shell.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    // The list has no task here, so the bar's state stands for it.
    let open = shell.read_with(cx, |shell, cx| shell.conversation.read(cx).find_open());
    assert!(open);
    // Inside a field the key stays the field's.
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell
                .conversation
                .update(cx, |conversation, cx| conversation.close_find(cx))
        })
    });
    let composer = shell.read_with(cx, |shell, cx| {
        let input = shell.conversation.read(cx).composer.read(cx).input.clone();
        gpui::Focusable::focus_handle(input.read(cx), cx)
    });
    cx.update(|window, cx| window.focus(&composer, cx));
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    let open = shell.read_with(cx, |shell, cx| shell.conversation.read(cx).find_open());
    assert!(!open, "the composer keeps ⌘F");
}

#[gpui::test]
fn the_columns_delivery_reaches_the_header_and_review_opens_changes(cx: &mut TestAppContext) {
    use crate::screens::conversation::header::delivery::DeliveryButtonsEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let repo = crate::test_support::git_checkout(&[("a.rs", "fn a() {}\n")]);
    std::fs::write(repo.path().join("a.rs"), "fn a() { b(); }\n").unwrap();
    let (column, conversation) = shell.read_with(cx, |shell, _| {
        (shell.code_column.clone(), shell.conversation.clone())
    });
    let root = repo.path().to_string_lossy().into_owned();
    // The open task's checkout: the shell gives it to the column.
    conversation.update(cx, |view, cx| {
        let data = crate::data::conversation::Conversation {
            task_id: "t".into(),
            workspace_path: Some(root),
            last_sequence: 1,
            ..Default::default()
        };
        view.show_loaded(data, cx);
    });
    cx.run_until_parked();
    assert!(column.read_with(cx, |column, _| column.root.is_some()));
    let delivery = conversation.read_with(cx, |view, cx| view.delivery.read(cx).controls.clone());
    let delivery = delivery.expect("the header has the task's delivery");
    assert_eq!(delivery.status.counts(), (1, 1));
    let buttons = conversation.read_with(cx, |view, _| view.delivery.clone());
    buttons.update(cx, |_, cx| cx.emit(DeliveryButtonsEvent::ReviewChanges));
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell.prefs.code_column_open);
        assert_eq!(shell.prefs.code_column_tab, CodeColumnTab::Changes);
    });
}

#[gpui::test]
fn the_browsers_page_context_lands_in_the_composers_draft(cx: &mut TestAppContext) {
    use crate::screens::browser::BrowserEvent;
    use crate::screens::browser::page::{FakePage, Page};
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let conversation = shell.read_with(cx, |shell, _| shell.conversation.clone());
    conversation.update(cx, |view, cx| {
        let data = crate::data::conversation::Conversation {
            task_id: "t".into(),
            session_id: Some("s".into()),
            // A task's checkout: the Browser stays open for its task.
            root_path: Some("/tmp/dcc-browser-test".into()),
            last_sequence: 1,
            ..Default::default()
        };
        view.show_loaded(data, cx);
    });
    cx.run_until_parked();
    shell.update_in(cx, |shell, window, cx| {
        shell.browser.pages = std::rc::Rc::new(|_, _, _| {
            Ok(std::rc::Rc::new(FakePage::default()) as std::rc::Rc<dyn Page>)
        });
        shell.open_browser(Some("https://example.com/".into()), window, cx);
    });
    let surface = shell.read_with(cx, |shell, _| {
        shell
            .browser
            .open
            .as_ref()
            .expect("the Browser")
            .surface
            .clone()
    });
    surface.update(cx, |_, cx| {
        cx.emit(BrowserEvent::SendToAgent(
            "<browser_page>\nGuia\n</browser_page>".into(),
        ))
    });
    cx.run_until_parked();
    let draft = cx.update(|_, cx| {
        let composer = conversation.read(cx).composer.clone();
        composer.read(cx).input.read(cx).text().to_string()
    });
    assert!(draft.contains("<browser_page>"), "{draft}");
}

#[gpui::test]
fn an_allowed_open_goes_to_its_conversation_and_opens_the_browser_there(cx: &mut TestAppContext) {
    use crate::data::browser_opens::OpenRequest;
    use crate::screens::browser::page::{FakePage, Page};
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let conversation = shell.read_with(cx, |shell, _| shell.conversation.clone());
    let loaded = |session: &str| crate::data::conversation::Conversation {
        task_id: "t".into(),
        session_id: Some(session.into()),
        root_path: Some("/tmp/dcc-browser-test".into()),
        last_sequence: 1,
        ..Default::default()
    };
    conversation.update(cx, |view, cx| view.show_loaded(loaded("s-1"), cx));
    cx.run_until_parked();
    shell.update_in(cx, |shell, window, cx| {
        shell.browser.pages = std::rc::Rc::new(|_, _, _| {
            Ok(std::rc::Rc::new(FakePage::default()) as std::rc::Rc<dyn Page>)
        });
        shell.open_approved(
            OpenRequest {
                request_id: "r-1".into(),
                workspace_id: "t".into(),
                session_id: "s-2".into(),
                url: "https://example.com/plans".into(),
                ..OpenRequest::default()
            },
            window,
            cx,
        );
    });
    // The conversation's own read settles first (it would replace what the
    // test shows next).
    cx.run_until_parked();
    // Another conversation of the task: nothing opens until it shows.
    assert!(!shell.read_with(cx, |shell, _| shell.browser_open()));
    conversation.update(cx, |view, cx| view.show_loaded(loaded("s-2"), cx));
    cx.run_until_parked();
    let (url, approval) = shell.read_with(cx, |shell, cx| {
        let open = shell.browser.open.as_ref().expect("the Browser");
        let surface = open.surface.read(cx);
        (
            surface.request.initial_url.clone(),
            open.approval
                .as_ref()
                .map(|request| request.request_id.clone()),
        )
    });
    assert_eq!(url.as_deref(), Some("https://example.com/plans"));
    assert_eq!(approval.as_deref(), Some("r-1"));
    assert!(shell.read_with(cx, |shell, _| shell.browser.waiting.is_empty()));
    // The desktop knows that Browser: the open is confirmed once.
    let surface = shell.read_with(cx, |shell, _| {
        shell.browser.open.as_ref().unwrap().surface.clone()
    });
    surface.update(cx, |_, cx| {
        cx.emit(crate::screens::browser::BrowserEvent::Ready(41))
    });
    cx.run_until_parked();
    assert!(shell.read_with(cx, |shell, _| {
        shell.browser.open.as_ref().unwrap().approval.is_none()
    }));
}

#[gpui::test]
fn the_sidebars_help_opens_and_its_footer_swaps_it_for_the_shortcuts(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenHelp));
    cx.run_until_parked();
    assert!(cx.debug_bounds("help-dialog").is_some());
    let bounds = cx.debug_bounds("help-open-shortcuts").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("help-dialog").is_none());
    assert!(cx.debug_bounds("shortcuts-dialog").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("shortcuts-dialog").is_none());
}

#[gpui::test]
fn cmd_p_and_cmd_shift_f_open_a_checkout_file_on_the_stage(cx: &mut TestAppContext) {
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    cx.update(|_, cx| {
        super::bind_keys(cx);
        crate::ui::text_input::bind_keys(cx);
    });
    // No task open: nothing to look in.
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    assert!(cx.debug_bounds("quick-open-field").is_none());
    let repo = crate::test_support::git_checkout(&[
        ("src/a.rs", "fn a() {}\nlet needle = 1;\n"),
        ("b.rs", "b\n"),
    ]);
    let root = repo.path().to_string_lossy().into_owned();
    let conversation = shell.read_with(cx, |shell, _| shell.conversation.clone());
    conversation.update(cx, |view, cx| {
        let data = crate::data::conversation::Conversation {
            task_id: "t".into(),
            workspace_path: Some(root),
            last_sequence: 1,
            ..Default::default()
        };
        view.show_loaded(data, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
    assert!(cx.debug_bounds("quick-open-field").is_some());
    cx.simulate_input("b.rs");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("quick-open-field").is_none());
    let stage = shell.read_with(cx, |shell, _| shell.stage.clone());
    stage.read_with(cx, |stage, _| {
        assert_eq!(stage.files.as_ref().unwrap().active().unwrap().path, "b.rs")
    });
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    cx.simulate_input("needle");
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("workspace-search-field").is_none());
    stage.read_with(cx, |stage, _| {
        assert_eq!(
            stage.files.as_ref().unwrap().active().unwrap().path,
            "src/a.rs"
        )
    });
}

#[gpui::test]
fn cmd_k_opens_the_palette_and_runs_what_is_picked(cx: &mut TestAppContext) {
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    cx.update(|_, cx| {
        super::bind_keys(cx);
        crate::ui::text_input::bind_keys(cx);
    });
    // No task open: the palette still opens, with the global actions.
    cx.simulate_keystrokes("cmd-k");
    cx.run_until_parked();
    assert!(cx.debug_bounds("command-palette-field").is_some());
    cx.simulate_input("atalhos");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(cx.debug_bounds("command-palette-field").is_none());
    assert!(cx.debug_bounds("shortcuts-dialog").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    // With a task: "Abrir Arquivos" shows the code column on "Arquivos".
    let repo = crate::test_support::git_checkout(&[("a.rs", "fn a() {}\n")]);
    let conversation = shell.read_with(cx, |shell, _| shell.conversation.clone());
    conversation.update(cx, |view, cx| {
        let data = crate::data::conversation::Conversation {
            task_id: "t".into(),
            workspace_path: Some(repo.path().to_string_lossy().into_owned()),
            last_sequence: 1,
            ..Default::default()
        };
        view.show_loaded(data, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    let label = cx.update(|_, cx| {
        crate::i18n::Translate::t(cx, "commandPalette.workbench.inspectorFiles").to_string()
    });
    cx.simulate_input(&label.to_lowercase());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert!(shell.prefs.code_column_open);
        assert_eq!(shell.prefs.code_column_tab, CodeColumnTab::Files);
    });
}

#[gpui::test]
fn the_sidebars_chart_button_opens_the_usage(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenUsage));
    cx.run_until_parked();
    assert!(cx.debug_bounds("usage-dialog").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("usage-dialog").is_none());
}

#[gpui::test]
fn the_sidebars_speech_bubble_opens_the_feedback(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenFeedback));
    cx.run_until_parked();
    assert!(cx.debug_bounds("feedback-dialog").is_some());
    // No GitHub account here: it asks to connect.
    assert!(cx.debug_bounds("feedback-connect-hint").is_some());
}

#[gpui::test]
fn the_sidebars_sparkles_open_the_skills(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, _dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenSkills));
    cx.run_until_parked();
    assert!(cx.debug_bounds("skills-dialog").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("skills-dialog").is_none());
}

#[gpui::test]
fn settings_apply_the_theme_language_and_density_at_once(cx: &mut TestAppContext) {
    use crate::screens::sidebar::SidebarEvent;
    let (shell, cx, dir) = open_shell(cx, Prefs::default());
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar.clone());
    sidebar.update(cx, |_, cx| cx.emit(SidebarEvent::OpenSettings));
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-dialog").is_some());
    for selector in [
        "settings-section-appearance",
        "settings-theme-dark",
        "settings-locale-1",
        "settings-density-1",
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        use crate::theme::ActiveTheme;
        assert_eq!(cx.theme().preference, crate::theme::ThemePreference::Dark);
        assert_eq!(
            cx.global::<crate::i18n::I18n>().locale(),
            crate::i18n::Locale::En
        );
        assert_eq!(
            crate::theme::Density::current(cx),
            crate::theme::Density::Compact
        );
    });
    let saved = Prefs::load(&dir.path().join("native-ui.json"));
    assert_eq!(saved.theme, crate::theme::ThemePreference::Dark);
    assert_eq!(saved.locale, crate::i18n::Locale::En);
    assert_eq!(saved.density, crate::theme::Density::Compact);
}
