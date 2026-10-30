//! A task the desktop just created opens even when the periodic read
//! listed it before the desktop answered (the list then reads the same).

use gpui::{AppContext, TestAppContext, WindowAppearance};

use super::{Sidebar, SidebarEvent};
use crate::data::loader::SidebarData;
use crate::data::tasks::TaskSummary;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn a_created_task_already_listed_still_opens(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let seen = dir.path().join("seen.json");
    let sidebar = cx.new(|cx| Sidebar::new(false, false, false, seen, cx));
    cx.run_until_parked();
    let data = SidebarData {
        tasks: vec![TaskSummary::test("w-old"), TaskSummary::test("w-new")],
        ..SidebarData::default()
    };
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = opened.clone();
    sidebar.update(cx, |sidebar, cx| {
        cx.subscribe_self(move |_, event: &SidebarEvent, _| {
            if let SidebarEvent::OpenTask(id) = event {
                seen.borrow_mut().push(id.clone());
            }
        })
        .detach();
        sidebar.apply_for_tests(data.clone(), cx);
        sidebar.reveal_task("w-old", cx);
    });
    opened.borrow_mut().clear();
    // The periodic read already has it; the desktop's answer comes after.
    sidebar.update(cx, |sidebar, cx| {
        sidebar.open_when_listed("w-new".into(), cx);
        sidebar.apply_for_tests(data, cx);
    });
    assert_eq!(opened.borrow().as_slice(), ["w-new"]);
    sidebar.read_with(cx, |sidebar, _| {
        assert_eq!(sidebar.state.selected_task_id.as_deref(), Some("w-new"));
    });
}
