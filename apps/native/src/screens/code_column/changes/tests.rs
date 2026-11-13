//! "Mudanças" over a real checkout: the groups and cards from git status,
//! the first diffs loaded, cards opened and closed, the wheel kept inside a
//! card's diff while it scrolls.

use gpui::{
    Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, WindowAppearance, point, px, size,
};

use super::super::{CodeColumn, CodeColumnTab};
use super::{CardDiff, ChangeGroup, ChangesRow, Scope, card_diff_height, summarize};
use crate::data::desktop_api::DesktopApi;
use crate::test_support::{git, git_checkout};
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<CodeColumn>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(|_, cx| {
        let mut column = CodeColumn::new(CodeColumnTab::Changes, cx);
        // A database of its own: no task, so the branch's base comes from git.
        column.db_path = DB.with(|dir| dir.path().join("database.sqlite"));
        column
    })
}

thread_local! {
    static DB: tempfile::TempDir = tempfile::tempdir().unwrap();
}

fn lines(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

#[gpui::test]
fn changes_list_the_staged_and_unstaged_files_as_cards(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("src/lib.rs", &lines(60)), ("README.md", "hi\n")]);
    std::fs::write(
        repo.path().join("src/lib.rs"),
        lines(60).replace("line 30\n", "line thirty\n"),
    )
    .unwrap();
    std::fs::write(repo.path().join("README.md"), "hello\n").unwrap();
    git(repo.path(), &["add", "README.md"]);
    std::fs::write(repo.path().join("new.txt"), "a\nb\n").unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root), cx));
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(900.0)));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let cards: Vec<(ChangeGroup, &str, &str)> = column
            .changes
            .cards
            .iter()
            .map(|card| {
                (
                    card.group,
                    card.entry.path.as_str(),
                    card.entry.status.as_str(),
                )
            })
            .collect();
        assert_eq!(
            cards,
            vec![
                (ChangeGroup::Staged, "README.md", "M"),
                (ChangeGroup::Unstaged, "new.txt", "?"),
                (ChangeGroup::Unstaged, "src/lib.rs", "M"),
            ]
        );
        assert_eq!(
            column.changes.rows(),
            vec![
                ChangesRow::Group(ChangeGroup::Staged),
                ChangesRow::Card(0),
                ChangesRow::Group(ChangeGroup::Unstaged),
                ChangesRow::Card(1),
                ChangesRow::Card(2),
            ]
        );
        // Three files read straight through: every card open, every diff in.
        assert!(column.changes.cards_open(Scope::Working));
        assert!(
            column
                .changes
                .cards
                .iter()
                .all(|card| matches!(card.diff, CardDiff::Ready(_))),
            "the diffs loaded"
        );
        let summary = summarize(column.changes.cards.iter().map(|card| &card.entry));
        // git counts nothing for a file it doesn't track yet.
        assert_eq!(
            (summary.files, summary.insertions, summary.deletions),
            (3, 2, 2)
        );
    });
    assert!(cx.debug_bounds("changes-scroll").is_some());
    assert!(cx.debug_bounds("diff-view").is_some());
    // A card closes on its header; a group folds its cards away.
    let header = cx.debug_bounds("change-card-0").unwrap();
    cx.simulate_click(header.center(), Modifiers::none());
    column.read_with(cx, |column, _| assert!(!column.changes.cards[0].open));
    let staged = cx.debug_bounds("changes-group-staged").unwrap();
    cx.simulate_click(staged.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(
            column.changes.rows()[1],
            ChangesRow::Group(ChangeGroup::Unstaged)
        );
    });
}

#[gpui::test]
fn a_long_review_starts_closed_and_the_header_opens_it(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> = (1..=5)
        .map(|n| (format!("f{n}.txt"), "one\n".to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let repo = git_checkout(&refs);
    for (path, _) in &files {
        std::fs::write(repo.path().join(path), "two\n").unwrap();
    }
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root.clone()), cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.changes.cards.len(), 5);
        assert!(column.changes.cards.iter().all(|card| !card.open));
        assert!(
            column
                .changes
                .cards
                .iter()
                .all(|card| matches!(card.diff, CardDiff::Idle)),
            "closed cards wait"
        );
    });
    column.update(cx, |column, cx| column.toggle_all_cards(cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert!(column.changes.cards.iter().all(|card| card.open));
        assert!(matches!(column.changes.cards[0].diff, CardDiff::Ready(_)));
        assert!(matches!(column.changes.cards[1].diff, CardDiff::Ready(_)));
    });
    // A new status keeps the cards' state; a changed count reads the diff again.
    std::fs::write(repo.path().join("f1.txt"), "two\nthree\n").unwrap();
    column.update(cx, |column, cx| {
        let status = dcc_infra::workspace_git::read_workspace_git_status(&root).unwrap();
        column.apply_status(status, cx);
        assert!(column.changes.cards.iter().all(|card| card.open));
        assert!(matches!(column.changes.cards[0].diff, CardDiff::Idle));
        assert!(matches!(column.changes.cards[1].diff, CardDiff::Ready(_)));
    });
}

#[gpui::test]
fn the_wheel_scrolls_a_long_diff_before_the_list(cx: &mut TestAppContext) {
    let original = lines(400);
    let modified: String = original
        .lines()
        .map(|line| format!("{line} changed\n"))
        .collect();
    let repo = git_checkout(&[("big.txt", &original), ("small.txt", "a\n")]);
    std::fs::write(repo.path().join("big.txt"), modified).unwrap();
    std::fs::write(repo.path().join("small.txt"), "b\n").unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root), cx));
    cx.run_until_parked();
    // Tall enough for the delivery header and the card's top.
    cx.simulate_resize(size(px(340.0), px(520.0)));
    cx.run_until_parked();
    let body = cx
        .debug_bounds("change-card-body-0")
        .expect("the big diff painted");
    // Inside the window: the card runs past its bottom.
    let inside = body.origin + point(px(40.0), px(60.0));
    let scroll = |cx: &mut gpui::VisualTestContext, dy: f32| {
        cx.simulate_mouse_move(inside, None, Modifiers::none());
        cx.simulate_event(ScrollWheelEvent {
            position: inside,
            delta: ScrollDelta::Pixels(point(px(0.0), px(dy))),
            ..Default::default()
        });
        cx.run_until_parked();
    };
    let page = |cx: &mut gpui::VisualTestContext| {
        column.read_with(cx, |column, _| {
            let top = column.changes.list.logical_scroll_top();
            (top.item_ix, f32::from(top.offset_in_item))
        })
    };
    scroll(cx, -60.0);
    assert_eq!(page(cx), (0, 0.0), "the diff took the wheel");
    let diff_view = column.read_with(cx, |column, _| match &column.changes.cards[0].diff {
        CardDiff::Ready(view) => view.clone(),
        _ => panic!("big.txt first"),
    });
    diff_view.read_with(cx, |view, _| assert!(view.list_top() != (0, px(0.0))));
    // Back at the diff's top the wheel goes on to the list.
    column.update(cx, |column, _| {
        column.changes.list.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: px(80.0),
        })
    });
    cx.run_until_parked();
    let parked = page(cx);
    assert_ne!(parked, (0, 0.0));
    scroll(cx, 60.0);
    diff_view.read_with(cx, |view, _| assert_eq!(view.list_top(), (0, px(0.0))));
    assert_eq!(page(cx), parked, "the diff still moved");
    scroll(cx, 30.0);
    assert_ne!(page(cx), parked, "the diff at its top hands the wheel on");
    assert_eq!(card_diff_height(400, 400), 520.0);
}

/// `debug_bounds` takes a `'static` name; tests build them from indices.
fn selector(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

fn requests_of(
    server: std::thread::JoinHandle<Vec<(String, String)>>,
) -> Vec<(String, serde_json::Value)> {
    server
        .join()
        .unwrap()
        .into_iter()
        .map(|(line, body)| (line, serde_json::from_str(&body).unwrap()))
        .collect()
}

#[gpui::test]
fn the_git_actions_go_through_the_desktop(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n"), ("b.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "dois\n").unwrap();
    git(repo.path(), &["add", "b.txt"]);
    let root = repo.path().to_string_lossy().into_owned();
    let ok = || (200, r#"{"ok":true}"#.to_string());
    let (port, server) = crate::test_support::fake_desktop_routes(vec![
        ("/api/v1/git/stage-file", ok().0, ok().1),
        ("/api/v1/git/unstage-file", ok().0, ok().1),
        ("/api/v1/git/unstage-file", ok().0, ok().1),
        ("/api/v1/git/discard-file", ok().0, ok().1),
        ("/api/v1/git/discard-file", ok().0, ok().1),
    ]);
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| {
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column.set_root(Some(root.clone()), cx);
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(900.0)));
    cx.run_until_parked();
    // The unstaged card stages; the staged one unstages.
    let unstaged = column.read_with(cx, |column, _| {
        column
            .changes
            .cards
            .iter()
            .position(|card| card.group == ChangeGroup::Unstaged)
            .unwrap()
    });
    let stage = cx
        .debug_bounds(selector(format!("change-card-stage-{unstaged}")))
        .expect("stage on the unstaged card");
    cx.simulate_click(stage.center(), Modifiers::none());
    cx.run_until_parked();
    let staged = cx
        .debug_bounds("change-card-unstage-0")
        .expect("unstage on the staged card");
    cx.simulate_click(staged.center(), Modifiers::none());
    cx.run_until_parked();
    // "Reverter tudo" asks first, then unstages the staged and discards all.
    let revert = cx
        .debug_bounds("changes-revert-all")
        .expect("the floating bar");
    cx.simulate_click(revert.center(), Modifiers::none());
    column.read_with(cx, |column, _| assert!(column.changes.confirm_revert));
    column.update(cx, |column, cx| column.revert_all(cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert!(!column.changes.busy);
        assert!(!column.changes.confirm_revert);
    });
    let requests = requests_of(server);
    let routes: Vec<(&str, &str)> = requests
        .iter()
        .map(|(line, body)| {
            (
                line.split_whitespace().nth(1).unwrap(),
                body["relativePath"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        routes,
        vec![
            ("/api/v1/git/stage-file", "a.txt"),
            ("/api/v1/git/unstage-file", "b.txt"),
            ("/api/v1/git/unstage-file", "b.txt"),
            ("/api/v1/git/discard-file", "b.txt"),
            ("/api/v1/git/discard-file", "a.txt"),
        ]
    );
    assert!(
        requests
            .iter()
            .all(|(_, body)| body["workspaceRoot"] == root.as_str())
    );
}

#[gpui::test]
fn a_failed_action_says_so_and_frees_the_list(cx: &mut TestAppContext) {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let (port, server) = crate::test_support::fake_desktop_routes(vec![(
        "/api/v1/git/stage-file",
        400,
        r#"{"error":{"code":"bad_request","message":"git add: index.lock exists"}}"#.to_string(),
    )]);
    cx.update(crate::ui::Toaster::init);
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| {
        column.endpoint = Some(DesktopApi::new(port, "k"));
        column.set_root(Some(root), cx);
    });
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        column.git_file(crate::data::desktop_api::GitAction::Stage, "a.txt", cx);
        assert!(column.changes.busy, "busy while it runs");
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| assert!(!column.changes.busy));
    let toasts = cx.update(|_, cx| crate::ui::Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast.contains("index.lock exists")),
        "{toasts:?}"
    );
    server.join().unwrap();
}

/// A checkout on `feature`, one commit ahead of `origin/main`.
fn branch_checkout() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    let repo = dir.path().join("repo");
    git(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            remote.to_str().unwrap(),
        ],
    );
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "oi\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    git(&repo, &["switch", "-q", "-c", "feature"]);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/novo.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "novo"]);
    dir
}

#[gpui::test]
fn a_clean_checkout_opens_on_its_branch_against_the_base(cx: &mut TestAppContext) {
    let dir = branch_checkout();
    let repo = dir.path().join("repo");
    let root = repo.to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root.clone()), cx));
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(900.0)));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        // Nothing local, one file on the branch: "Branch" by default.
        assert_eq!(column.changes.scope(), Scope::Branch);
        assert_eq!(column.changes.branch_count(), Some(1));
        assert_eq!(column.changes.base_branch().as_deref(), Some("origin/main"));
        assert_eq!(
            column.changes.rows(),
            vec![
                ChangesRow::BranchHeader,
                ChangesRow::Space(4),
                ChangesRow::Card(0),
                ChangesRow::Space(8),
            ]
        );
        let card = &column.changes.cards[0];
        assert_eq!(
            (card.group, card.entry.path.as_str()),
            (ChangeGroup::Committed, "src/novo.rs")
        );
        assert!(
            matches!(card.diff, CardDiff::Ready(_)),
            "read against the base"
        );
    });
    assert!(cx.debug_bounds("changes-scope-branch").is_some());
    // "Workspace" shows the empty state; the pick stays for this checkout.
    let working = cx.debug_bounds("changes-scope-working").unwrap();
    cx.simulate_click(working.center(), Modifiers::none());
    column.read_with(cx, |column, _| {
        assert_eq!(column.changes.scope(), Scope::Working);
        assert_eq!(column.changes.rows(), vec![ChangesRow::Empty]);
    });
    // A file new to the branch shimmers.
    column.update(cx, |column, cx| column.set_scope(Scope::Branch, cx));
    std::fs::write(repo.join("src/outro.rs"), "fn x() {}\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "outro"]);
    column.update(cx, |column, cx| column.read_branch(cx));
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.changes.branch_count(), Some(2));
        assert!(column.changes.is_flashing("src/outro.rs"));
        assert!(!column.changes.is_flashing("src/novo.rs"));
    });
}

#[gpui::test]
fn the_tree_folds_folders_and_shows_a_files_actions_on_hover(cx: &mut TestAppContext) {
    let repo = git_checkout(&[
        ("src/a.rs", "1\n"),
        ("src/ui/b.rs", "1\n"),
        ("README.md", "1\n"),
    ]);
    for path in ["src/a.rs", "src/ui/b.rs", "README.md"] {
        std::fs::write(repo.path().join(path), "2\n").unwrap();
    }
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root), cx));
    cx.run_until_parked();
    let tree = |column: &CodeColumn| -> Vec<String> {
        column
            .changes
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                ChangesRow::Folder {
                    name, depth, open, ..
                } => Some(format!(
                    "{}{name}/{}",
                    "  ".repeat(depth),
                    if open { "" } else { "+" }
                )),
                ChangesRow::File { card, depth } => Some(format!(
                    "{}{}",
                    "  ".repeat(depth),
                    column.changes.cards[card].entry.name
                )),
                _ => None,
            })
            .collect()
    };
    let tree_button = cx
        .debug_bounds("changes-toggle-tree")
        .expect("list or tree");
    cx.simulate_click(tree_button.center(), Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(
            tree(column),
            vec!["src/", "  ui/", "    b.rs", "  a.rs", "README.md"]
        );
    });
    column.update(cx, |column, cx| {
        column.toggle_changes_folder(ChangeGroup::Unstaged, "src/ui", cx)
    });
    column.read_with(cx, |column, _| {
        assert_eq!(tree(column), vec!["src/", "  ui/+", "  a.rs", "README.md"]);
    });
    cx.run_until_parked();
    let readme = column.read_with(cx, |column, _| {
        column
            .changes
            .cards
            .iter()
            .position(|card| card.entry.path == "README.md")
            .unwrap()
    });
    let row = cx
        .debug_bounds(selector(format!("changes-file-{readme}")))
        .unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::none());
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        assert_eq!(column.changes.hovered_file, Some(readme))
    });
    assert!(
        cx.debug_bounds(selector(format!("changes-file-discard-{readme}")))
            .is_some()
    );
    assert!(
        cx.debug_bounds(selector(format!("changes-file-stage-{readme}")))
            .is_some()
    );
}

#[gpui::test]
fn only_the_cards_near_the_viewport_read_their_diff(cx: &mut TestAppContext) {
    let files: Vec<(String, String)> = (1..=60)
        .map(|n| (format!("f{n:02}.txt"), "one\n".to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect();
    let repo = git_checkout(&refs);
    for (path, _) in &files {
        std::fs::write(repo.path().join(path), "two\n").unwrap();
    }
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root), cx));
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(600.0)));
    column.update(cx, |column, cx| column.toggle_all_cards(cx));
    cx.run_until_parked();
    let read = column.read_with(cx, |column, _| {
        column
            .changes
            .cards
            .iter()
            .filter(|card| !matches!(card.diff, CardDiff::Idle))
            .count()
    });
    // Cards of ~240px: the viewport and 700px past it, not all sixty.
    assert!((2..12).contains(&read), "{read} diffs read");
    assert!(cx.debug_bounds("change-card-0").is_some());
    assert!(
        cx.debug_bounds("change-card-59").is_none(),
        "far cards aren't drawn"
    );
}

#[gpui::test]
fn a_card_and_a_tree_row_open_their_diff_on_the_stage(cx: &mut TestAppContext) {
    use super::super::CodeColumnEvent;
    use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;
    let repo = git_checkout(&[("src/lib.rs", "a\n")]);
    std::fs::write(repo.path().join("src/lib.rs"), "b\n").unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    column.update(cx, |column, cx| {
        cx.subscribe_self(move |_, event: &CodeColumnEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
        column.set_root(Some(root.clone()), cx);
    });
    cx.run_until_parked();
    cx.simulate_resize(size(px(340.0), px(900.0)));
    cx.run_until_parked();
    let expand = cx
        .debug_bounds("change-card-expand-0")
        .expect("Abrir diff ampliado");
    cx.simulate_click(expand.center(), Modifiers::none());
    let opened = |events: &[CodeColumnEvent]| -> Vec<(String, WorkspaceGitPreviewScope, bool)> {
        events
            .iter()
            .filter_map(|event| match event {
                CodeColumnEvent::OpenDiff(request) => Some((
                    request.path.clone(),
                    request.scope.clone(),
                    request.from_changes,
                )),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        opened(&events.borrow()),
        vec![(
            "src/lib.rs".into(),
            WorkspaceGitPreviewScope::Unstaged,
            true
        )]
    );
    // In the tree the row opens it, and the one on the stage is marked.
    column.update(cx, |column, cx| {
        column.toggle_changes_tree(cx);
        column.set_open_diff(
            Some((WorkspaceGitPreviewScope::Unstaged, "src/lib.rs".into())),
            cx,
        );
    });
    cx.run_until_parked();
    let row = cx.debug_bounds("changes-file-0").expect("the tree row");
    cx.simulate_click(row.center(), Modifiers::none());
    assert_eq!(opened(&events.borrow()).len(), 2);
}
