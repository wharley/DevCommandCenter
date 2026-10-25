//! The stage driven headlessly: a change's diff on it, the Chat tab and
//! back, Esc, the agent's work marked on the Chat tab, and closing.

use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};

use super::{DiffLoad, DiffRequest, ShowChat, Stage, StageEvent};
use crate::test_support::git_checkout;
use crate::theme::{Theme, ThemePreference};

fn request(root: &std::path::Path) -> DiffRequest {
    DiffRequest {
        root: root.to_string_lossy().into_owned(),
        path: "src/main.rs".into(),
        name: "main.rs".into(),
        status: "M".into(),
        scope: WorkspaceGitPreviewScope::Unstaged,
        base: None,
        from_changes: true,
    }
}

fn open(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<Stage>,
    &mut gpui::VisualTestContext,
    tempfile::TempDir,
    std::rc::Rc<std::cell::RefCell<Vec<StageEvent>>>,
) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        super::bind_keys(cx);
    });
    let repo = git_checkout(&[("src/main.rs", "fn main() {}\n")]);
    std::fs::write(repo.path().join("src/main.rs"), "fn main() { run(); }\n").unwrap();
    let (stage, cx) = cx.add_window_view(|_, cx| Stage::new(cx));
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    stage.update(cx, |_, cx| {
        cx.subscribe_self(move |_, event: &StageEvent, _| seen.borrow_mut().push(event.clone()))
            .detach();
    });
    cx.simulate_resize(size(px(900.0), px(700.0)));
    (stage, cx, repo, events)
}

#[gpui::test]
fn a_change_opens_over_the_chat_and_the_chat_tab_shrinks_it(cx: &mut TestAppContext) {
    let (stage, cx, repo, events) = open(cx);
    let request = request(repo.path());
    stage.update_in(cx, |stage, window, cx| stage.open_diff(request, window, cx));
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| {
        assert!(stage.has_content());
        assert!(!stage.chat_active());
        let diff = stage.diff.as_ref().unwrap();
        assert!(matches!(diff.load, DiffLoad::Ready(_)), "the change's diff");
    });
    assert!(cx.debug_bounds("diff-view").is_some());
    // The Chat tab: only the strip stays.
    let chat = cx.debug_bounds("stage-chat").unwrap();
    cx.simulate_click(chat.center(), Modifiers::none());
    stage.read_with(cx, |stage, _| assert!(stage.chat_active()));
    // Back to the diff, then Esc goes to the Chat again.
    let tab = cx.debug_bounds("stage-diff-tab").unwrap();
    cx.simulate_click(tab.center(), Modifiers::none());
    stage.read_with(cx, |stage, _| assert!(!stage.chat_active()));
    cx.dispatch_action(ShowChat);
    stage.read_with(cx, |stage, _| assert!(stage.chat_active()));
    // Closing tells where to go back.
    stage.update(cx, |stage, cx| stage.close(cx));
    stage.read_with(cx, |stage, _| assert!(!stage.has_content()));
    assert_eq!(
        events.borrow().as_slice(),
        &[StageEvent::Closed { to_changes: true }]
    );
}

#[gpui::test]
fn a_turn_that_ends_behind_the_content_marks_the_chat_tab(cx: &mut TestAppContext) {
    let (stage, cx, repo, _) = open(cx);
    let request = request(repo.path());
    stage.update_in(cx, |stage, window, cx| stage.open_diff(request, window, cx));
    cx.run_until_parked();
    stage.update(cx, |stage, cx| stage.set_agent_working(true, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("stage-chat-working").is_some());
    stage.update(cx, |stage, cx| stage.set_agent_working(false, cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("stage-chat-unread").is_some());
    // Seen once the Chat comes forward.
    stage.update(cx, |stage, cx| stage.show_chat(cx));
    stage.read_with(cx, |stage, _| assert!(!stage.chat_unread));
}

#[gpui::test]
fn a_missing_file_says_so(cx: &mut TestAppContext) {
    let (stage, cx, repo, _) = open(cx);
    let mut request = request(repo.path());
    request.root = repo.path().join("nowhere").to_string_lossy().into_owned();
    stage.update_in(cx, |stage, window, cx| stage.open_diff(request, window, cx));
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| {
        assert!(matches!(
            stage.diff.as_ref().unwrap().load,
            DiffLoad::Failed(_)
        ));
    });
}

#[gpui::test]
fn a_modified_file_reads_side_by_side_and_a_new_one_in_one_column(cx: &mut TestAppContext) {
    let (stage, cx, repo, _) = open(cx);
    let modified = request(repo.path());
    stage.update_in(cx, |stage, window, cx| {
        stage.open_diff(modified, window, cx)
    });
    cx.run_until_parked();
    let split = |stage: &gpui::Entity<Stage>, cx: &mut gpui::VisualTestContext| {
        stage.read_with(cx, |stage, cx| match &stage.diff.as_ref().unwrap().load {
            DiffLoad::Ready(view) => view.read(cx).split,
            _ => panic!("ready"),
        })
    };
    assert!(split(&stage, cx), "modified: side by side");
    std::fs::write(repo.path().join("new.rs"), "fn new() {}\n").unwrap();
    let mut added = request(repo.path());
    added.path = "new.rs".into();
    added.name = "new.rs".into();
    added.status = "?".into();
    stage.update_in(cx, |stage, window, cx| stage.open_diff(added, window, cx));
    cx.run_until_parked();
    assert!(!split(&stage, cx), "new: one column");
}

fn open_files(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<Stage>,
    &mut gpui::VisualTestContext,
    tempfile::TempDir,
    std::rc::Rc<std::cell::RefCell<Vec<StageEvent>>>,
) {
    let (stage, cx, repo, events) = open(cx);
    std::fs::write(repo.path().join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(repo.path().join("b.rs"), "fn b() {}\n").unwrap();
    std::fs::write(repo.path().join("c.rs"), "fn c() {}\n").unwrap();
    (stage, cx, repo, events)
}

fn tab_names(stage: &gpui::Entity<Stage>, cx: &mut gpui::VisualTestContext) -> Vec<(String, bool)> {
    stage.read_with(cx, |stage, _| {
        stage
            .files
            .as_ref()
            .map(|tabs| {
                tabs.files
                    .iter()
                    .map(|file| (file.path.clone(), file.preview))
                    .collect()
            })
            .unwrap_or_default()
    })
}

#[gpui::test]
fn files_open_as_previews_that_pin_and_close_to_a_neighbour(cx: &mut TestAppContext) {
    let (stage, cx, repo, events) = open_files(cx);
    let root = repo.path().to_string_lossy().into_owned();
    let open = |stage: &gpui::Entity<Stage>, cx: &mut gpui::VisualTestContext, path: &str| {
        let (root, path) = (root.clone(), path.to_string());
        stage.update_in(cx, |stage, window, cx| {
            stage.open_file(root, path, None, window, cx)
        });
        cx.run_until_parked();
    };
    open(&stage, cx, "a.rs");
    stage.read_with(cx, |stage, _| {
        let file = stage.files.as_ref().unwrap().active().unwrap();
        assert!(matches!(file.load, Some(super::files::FileLoad::Text(_))));
    });
    assert!(cx.debug_bounds("file-view").is_some());
    // The next preview takes the preview's place.
    open(&stage, cx, "b.rs");
    assert_eq!(tab_names(&stage, cx), vec![("b.rs".into(), true)]);
    // A double-click pins it; the next file opens beside it.
    let tab = cx.debug_bounds("stage-file-tab-0").unwrap();
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    stage.update(cx, |stage, cx| stage.pin_file(0, cx));
    open(&stage, cx, "c.rs");
    assert_eq!(
        tab_names(&stage, cx),
        vec![("b.rs".into(), false), ("c.rs".into(), true)]
    );
    // Only the active tab keeps its file read.
    stage.read_with(cx, |stage, _| {
        let tabs = stage.files.as_ref().unwrap();
        assert_eq!(tabs.active, 1);
        assert!(tabs.files[0].load.is_none());
    });
    // Closing the active tab brings its neighbour; the last closes the files.
    stage.update_in(cx, |stage, window, cx| stage.close_file(1, window, cx));
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| {
        let tabs = stage.files.as_ref().unwrap();
        assert_eq!(tabs.active().unwrap().path, "b.rs");
        assert!(tabs.active().unwrap().load.is_some(), "read again");
    });
    stage.update_in(cx, |stage, window, cx| stage.close_file(0, window, cx));
    stage.read_with(cx, |stage, _| assert!(!stage.has_content()));
    assert_eq!(
        events.borrow().as_slice(),
        &[StageEvent::Closed { to_changes: false }]
    );
}

#[gpui::test]
fn an_image_shows_as_a_picture_and_a_missing_file_says_so(cx: &mut TestAppContext) {
    let (stage, cx, repo, _) = open_files(cx);
    let picture = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
    picture.save(repo.path().join("shot.png")).unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let read_root = root.clone();
    stage.update_in(cx, |stage, window, cx| {
        stage.open_file(read_root, "shot.png".into(), None, window, cx)
    });
    cx.run_until_parked();
    stage.read_with(cx, |stage, cx| {
        let file = stage.files.as_ref().unwrap().active().unwrap();
        let Some(super::files::FileLoad::Image(view)) = &file.load else {
            panic!("an image tab");
        };
        assert!(matches!(
            view.read(cx).picture,
            super::image::Picture::Ready {
                width: 4,
                height: 3,
                ..
            }
        ));
    });
    assert!(cx.debug_bounds("image-view-bar").is_some());
    stage.update_in(cx, |stage, window, cx| {
        stage.open_file(root, "gone.rs".into(), None, window, cx)
    });
    cx.run_until_parked();
    stage.read_with(cx, |stage, _| {
        let file = stage.files.as_ref().unwrap().active().unwrap();
        assert!(matches!(file.load, Some(super::files::FileLoad::Failed(_))));
    });
}

#[gpui::test]
fn a_diff_replaces_the_files_and_a_file_replaces_the_diff(cx: &mut TestAppContext) {
    let (stage, cx, repo, _) = open_files(cx);
    let root = repo.path().to_string_lossy().into_owned();
    stage.update_in(cx, |stage, window, cx| {
        stage.open_file(root.clone(), "a.rs".into(), Some(1), window, cx)
    });
    cx.run_until_parked();
    let diff = request(repo.path());
    stage.update_in(cx, |stage, window, cx| stage.open_diff(diff, window, cx));
    stage.read_with(cx, |stage, _| {
        assert!(stage.files.is_none() && stage.diff.is_some())
    });
    stage.update_in(cx, |stage, window, cx| {
        stage.open_file(root, "a.rs".into(), None, window, cx)
    });
    stage.read_with(cx, |stage, _| {
        assert!(stage.diff.is_none() && stage.files.is_some())
    });
}
