//! The code column driven headlessly over a real checkout.

use gpui::{TestAppContext, WindowAppearance};

use super::{CodeColumn, CodeColumnEvent, CodeColumnTab, Loaded};
use crate::test_support::{git, git_checkout};
use crate::theme::{Theme, ThemePreference};

fn open(cx: &mut TestAppContext) -> (gpui::Entity<CodeColumn>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    cx.add_window_view(|_, cx| CodeColumn::new(CodeColumnTab::Files, cx))
}

#[gpui::test]
fn the_tree_lists_the_checkout_with_its_git_status(cx: &mut TestAppContext) {
    let repo = git_checkout(&[
        ("README.md", "hi"),
        ("apps/desktop/src/App.tsx", "a"),
        ("apps/desktop/package.json", "{}"),
        (".gitignore", ".env\n"),
    ]);
    std::fs::write(repo.path().join("README.md"), "changed").unwrap();
    std::fs::write(repo.path().join(".env"), "SECRET=1").unwrap();
    std::fs::write(repo.path().join("new.rs"), "fn main() {}").unwrap();
    let root = repo.path().to_string_lossy().into_owned();
    let (column, cx) = open(cx);
    column.update(cx, |column, cx| column.set_root(Some(root.clone()), cx));
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        let Loaded::Ready(files) = &column.files else {
            panic!("files loaded: {:?}", column.files);
        };
        assert!(
            files.paths.contains(&"new.rs".to_string()),
            "new files show before staging"
        );
        assert!(files.ignored.contains(".env"));
        let labels: Vec<&str> = column.rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["apps/desktop", ".env", ".gitignore", "new.rs", "README.md"]
        );
        assert_eq!(column.by_path["README.md"].letter, 'M');
        assert_eq!(column.by_path["new.rs"].letter, 'U');
        assert_eq!(column.change_count(), 2);
        // Opening the folder shows what is inside, folders first.
        column.toggle_folder("apps/desktop", cx);
        let labels: Vec<&str> = column.rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(&labels[..3], &["apps/desktop", "src", "package.json"]);
        column.toggle_folder("apps/desktop", cx);
        // Revealing a file opens the way down to it.
        column.reveal(Some("apps/desktop/src/App.tsx".into()), cx);
        assert!(
            column
                .rows
                .iter()
                .any(|row| row.path == "apps/desktop/src/App.tsx")
        );
    });
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(cx.debug_bounds("code-tree").is_some());
    assert!(cx.debug_bounds("code-column").is_some());
    // An agent deletes a file: the next status reads the list again.
    git(repo.path(), &["rm", "-q", "apps/desktop/package.json"]);
    column.update(cx, |column, cx| {
        let status = dcc_infra::workspace_git::read_workspace_git_status(&root).unwrap();
        column.apply_status(status, cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, _| {
        let Loaded::Ready(files) = &column.files else {
            panic!("files");
        };
        assert!(
            !files
                .paths
                .contains(&"apps/desktop/package.json".to_string())
        );
    });
}

#[gpui::test]
fn the_tabs_and_close_tell_the_shell(cx: &mut TestAppContext) {
    let (column, cx) = open(cx);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = events.clone();
    column.update(cx, |column, cx| {
        cx.subscribe_self(move |_, event: &CodeColumnEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
        column.set_tab(CodeColumnTab::Changes, cx);
        assert_eq!(column.tab.width(), 380.0);
        column.set_tab(CodeColumnTab::Changes, cx);
        column.open_file("a.rs", cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *events.borrow(),
        vec![
            CodeColumnEvent::TabChanged(CodeColumnTab::Changes),
            CodeColumnEvent::OpenFile("a.rs".into())
        ]
    );
}
