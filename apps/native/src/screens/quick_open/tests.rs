//! Quick Open over a real checkout: it lists the tracked files, typing
//! ranks them, ↓ and Enter pick, a click too, Esc closes.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{QuickOpen, QuickOpenEvent};
use crate::theme::{Theme, ThemePreference};

type Picked = Rc<RefCell<Vec<String>>>;

fn open(
    root: String,
    cx: &mut TestAppContext,
) -> (Entity<QuickOpen>, &mut VisualTestContext, Picked) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let (open, cx) = cx.add_window_view(move |window, cx| QuickOpen::new(root, window, cx));
    let picked: Picked = Rc::default();
    let sink = picked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&open, move |_, event: &QuickOpenEvent, _| {
            sink.borrow_mut().push(match event {
                QuickOpenEvent::Picked(path) => path.clone(),
                QuickOpenEvent::Closed => "closed".into(),
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (open, cx, picked)
}

#[gpui::test]
fn typing_ranks_the_checkouts_files_and_keys_pick(cx: &mut TestAppContext) {
    let repo = crate::test_support::git_checkout(&[
        ("src/main.rs", "fn main() {}\n"),
        ("src/shell/mod.rs", "\n"),
        ("README.md", "# x\n"),
    ]);
    let root = repo.path().to_string_lossy().into_owned();
    let (open, cx, picked) = open(root, cx);
    let listed = cx.update(|_, cx| open.read(cx).results.clone());
    assert_eq!(listed.len(), 3);
    assert!(cx.debug_bounds("quick-open-field").is_some());
    cx.simulate_input("mod");
    cx.run_until_parked();
    let ranked = cx.update(|_, cx| open.read(cx).results.clone());
    assert_eq!(ranked, ["src/shell/mod.rs"]);
    cx.simulate_input("x");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| open.read(cx).results.is_empty()));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("rs");
    cx.run_until_parked();
    // Two `.rs` files: ↓ then Enter picks the second; a click the first.
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    let second = cx.update(|_, cx| open.read(cx).results[1].clone());
    let first = cx.update(|_, cx| open.read(cx).results[0].clone());
    let bounds = cx.debug_bounds("quick-open-item-0").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        picked.borrow().as_slice(),
        [second, first, "closed".to_string()]
    );
}
