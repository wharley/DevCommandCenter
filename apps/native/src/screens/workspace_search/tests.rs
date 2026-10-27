//! The search over a real checkout: under two letters it only hints, then
//! `git grep` answers grouped by file, the hit marked; Enter opens a line.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use dcc_infra::workspace_search::{SearchMatch, SearchResults};
use gpui::{Entity, TestAppContext, VisualTestContext, WindowAppearance};

use super::view::first_hit;
use super::{WorkspaceSearch, WorkspaceSearchEvent, group};
use crate::theme::{Theme, ThemePreference};

#[test]
fn the_first_hit_ignores_case() {
    assert_eq!(first_hit("let Value = value;", "value"), Some(4..9));
    assert_eq!(first_hit("ação AÇÃO", "ação"), Some(0..6));
    assert_eq!(first_hit("nada", "x"), None);
    assert_eq!(first_hit("nada", ""), None);
}

#[test]
fn lines_group_by_file_in_order() {
    let line = |path: &str, line: u32| SearchMatch {
        path: path.into(),
        line,
        text: "x".into(),
    };
    let groups = group(SearchResults {
        matches: vec![line("a.rs", 1), line("b.rs", 4), line("a.rs", 9)],
        truncated: false,
    });
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].path, "a.rs");
    assert_eq!(groups[0].matches.len(), 2);
    assert_eq!(groups[1].path, "b.rs");
}

type Picked = Rc<RefCell<Vec<(String, u32)>>>;

fn open(
    root: String,
    cx: &mut TestAppContext,
) -> (Entity<WorkspaceSearch>, &mut VisualTestContext, Picked) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (search, cx) = cx.add_window_view(move |window, cx| WorkspaceSearch::new(root, window, cx));
    let picked: Picked = Rc::default();
    let sink = picked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&search, move |_, event: &WorkspaceSearchEvent, _| {
            if let WorkspaceSearchEvent::Picked { path, line } = event {
                sink.borrow_mut().push((path.clone(), *line));
            }
        })
        .detach();
    });
    cx.run_until_parked();
    (search, cx, picked)
}

#[gpui::test]
fn two_letters_search_the_checkout_and_enter_opens_the_line(cx: &mut TestAppContext) {
    let repo = crate::test_support::git_checkout(&[
        ("src/a.rs", "fn a() {}\nlet Needle = 1;\n"),
        ("b.rs", "needle here\n"),
    ]);
    let root = repo.path().to_string_lossy().into_owned();
    let (search, cx, picked) = open(root, cx);
    cx.simulate_input("n");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| !search.read(cx).has_query(cx)));
    cx.simulate_input("eedle");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let (files, total) = cx.update(|_, cx| {
        let search = search.read(cx);
        let files: Vec<String> = search
            .groups
            .iter()
            .map(|group| group.path.clone())
            .collect();
        (files, search.total())
    });
    assert_eq!(files.len(), 2, "{files:?}");
    assert_eq!(total, 2);
    assert!(cx.debug_bounds("workspace-search-item-1").is_some());
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    let expected = cx.update(|_, cx| {
        let found = search.read(cx).at(1).unwrap();
        (found.path.clone(), found.line)
    });
    assert_eq!(picked.borrow().as_slice(), [expected]);
    // The second file's line is its row after both headings.
    cx.update(|_, cx| assert_eq!(search.read(cx).row_of(1), 3));
}
