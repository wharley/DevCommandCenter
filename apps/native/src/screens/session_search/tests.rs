//! The dialog driven headlessly: an empty database says so, the open
//! task's conversations lead, ↓/↑ move the highlight, Enter and a click
//! pick, Esc closes.

use std::cell::RefCell;
use std::rc::Rc;

use dcc_core::domain::project::ProjectId;
use dcc_core::domain::session::{SessionId, SessionSearchResult};
use dcc_core::domain::workspace::WorkspaceId;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{SessionSearch, SessionSearchEvent};
use crate::theme::{Theme, ThemePreference};

fn result(session: &str, task: &str, archived: bool) -> SessionSearchResult {
    SessionSearchResult {
        session_id: SessionId(session.into()),
        workspace_id: WorkspaceId(task.into()),
        project_id: ProjectId("p".into()),
        thread_title: format!("Conversa {session}"),
        workspace_name: Some(format!("Tarefa {task}")),
        workspace_branch: Some("main".into()),
        workspace_root_path: None,
        provider_id: "codex".into(),
        model: None,
        archived_at: archived.then(|| "2026-10-24T10:00:00Z".to_string()),
        created_at: "2026-10-24T10:00:00Z".into(),
        updated_at: "2026-10-25T14:03:00Z".into(),
        snippet: "o checkout com retry".into(),
    }
}

type Picked = Rc<RefCell<Vec<String>>>;

fn open(cx: &mut TestAppContext) -> (Entity<SessionSearch>, &mut VisualTestContext, Picked) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    drop(dcc_infra::db::SqliteSessionRepo::open(&db).unwrap());
    let (search, cx) = cx.add_window_view(move |window, cx| {
        let _keep = &dir;
        SessionSearch::new(Some("t-1".into()), db.clone(), window, cx)
    });
    let picked: Picked = Rc::default();
    let sink = picked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&search, move |_, event: &SessionSearchEvent, _| {
            sink.borrow_mut().push(match event {
                SessionSearchEvent::Picked(result) => result.session_id.0.clone(),
                SessionSearchEvent::Closed => "closed".into(),
            })
        })
        .detach();
    });
    cx.run_until_parked();
    (search, cx, picked)
}

#[gpui::test]
fn the_open_tasks_conversations_lead_and_keys_pick(cx: &mut TestAppContext) {
    let (search, cx, picked) = open(cx);
    assert!(cx.update(|_, cx| search.read(cx).results.is_empty()));
    assert!(cx.debug_bounds("session-search-field").is_some());
    cx.update(|_, cx| {
        search.update(cx, |search, cx| {
            let ordered = search.ordered(vec![
                result("other", "t-2", false),
                result("mine", "t-1", true),
            ]);
            search.results = ordered;
            search.loading = false;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (first, current) = cx.update(|_, cx| {
        let search = search.read(cx);
        (
            search.results[0].session_id.0.clone(),
            search.current_count(),
        )
    });
    assert_eq!(first, "mine");
    assert_eq!(current, 1);
    assert!(cx.debug_bounds("session-result-0").is_some());
    assert!(cx.debug_bounds("session-result-1").is_some());
    // ↓ then Enter picks the second; a click picks the first.
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    let bounds = cx.debug_bounds("session-result-0").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(picked.borrow().as_slice(), ["other", "mine", "closed"]);
}
