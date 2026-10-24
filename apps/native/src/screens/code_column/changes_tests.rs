//! "Mudanças" over a real checkout: the groups and cards from git status,
//! the first diffs loaded, cards opened and closed, the wheel kept inside a
//! card's diff while it scrolls.

use gpui::{
    Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, WindowAppearance, point, px, size,
};

use super::changes::{CardDiff, ChangeGroup, card_diff_height, summarize};
use super::changes_view::ChangesRow;
use super::{CodeColumn, CodeColumnTab};
use crate::test_support::{git, git_checkout};
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<CodeColumn>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(|_, cx| CodeColumn::new(CodeColumnTab::Changes, cx))
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
            column.changes_rows(),
            vec![
                ChangesRow::Group(ChangeGroup::Staged),
                ChangesRow::Card(0),
                ChangesRow::Group(ChangeGroup::Unstaged),
                ChangesRow::Card(1),
                ChangesRow::Card(2),
            ]
        );
        // Three files read straight through: every card open, every diff in.
        assert!(column.changes.cards_open());
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
            column.changes_rows()[1],
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
    cx.simulate_resize(size(px(340.0), px(400.0)));
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
        column.read_with(cx, |column, _| f32::from(column.changes.scroll.offset().y))
    };
    scroll(cx, -60.0);
    assert_eq!(page(cx), 0.0, "the diff took the wheel");
    let diff_view = column.read_with(cx, |column, _| match &column.changes.cards[0].diff {
        CardDiff::Ready(view) => view.clone(),
        _ => panic!("big.txt first"),
    });
    diff_view.read_with(cx, |view, _| assert!(view.list_top() != (0, px(0.0))));
    // Back at the diff's top the wheel goes on to the list.
    column.update(cx, |column, _| {
        column.changes.scroll.set_offset(point(px(0.0), px(-80.0)))
    });
    cx.run_until_parked();
    scroll(cx, 60.0);
    diff_view.read_with(cx, |view, _| assert_eq!(view.list_top(), (0, px(0.0))));
    assert_eq!(page(cx), -80.0, "the diff still moved");
    scroll(cx, 30.0);
    assert_eq!(page(cx), -50.0, "the diff at its top hands the wheel on");
    assert_eq!(card_diff_height(400, 400), 520.0);
}
