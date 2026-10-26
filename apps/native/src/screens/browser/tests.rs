//! The Browser driven headlessly with a fake page: it opens on the URL
//! asked for, follows what the page reports, keeps the scope's last URL
//! (sanitized) for the next time, takes the omnibox, refuses what the
//! policy refuses, and hides under an overlay.

use std::cell::RefCell;
use std::rc::Rc;

use futures::channel::mpsc::UnboundedSender;
use gpui::{Entity, TestAppContext, VisualTestContext, WindowAppearance, px, size};

use super::page::{FakePage, Page, PageEvent};
use super::surface::PageFactory;
use super::{BrowserRequest, BrowserSurface};
use crate::theme::{Theme, ThemePreference};

#[derive(Default)]
struct Pages {
    page: Rc<FakePage>,
    opened_on: RefCell<Vec<String>>,
    events: RefCell<Option<UnboundedSender<PageEvent>>>,
}

fn factory(pages: &Rc<Pages>) -> PageFactory {
    let pages = pages.clone();
    Rc::new(move |_, url, events| {
        pages.opened_on.borrow_mut().push(url.to_string());
        *pages.events.borrow_mut() = Some(events);
        *pages.page.url.borrow_mut() = Some(url.to_string());
        Ok(pages.page.clone() as Rc<dyn Page>)
    })
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    db: &std::path::Path,
    initial_url: Option<&str>,
    pages: &Rc<Pages>,
) -> (Entity<BrowserSurface>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let request = BrowserRequest {
        workspace_id: "w-1".into(),
        session_id: Some("s-1".into()),
        initial_url: initial_url.map(str::to_string),
    };
    let make = factory(pages);
    let db = db.to_path_buf();
    let (surface, cx) =
        cx.add_window_view(move |window, cx| BrowserSurface::new(request, make, db, window, cx));
    cx.simulate_resize(size(px(900.0), px(700.0)));
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    (surface, cx)
}

fn send(pages: &Pages, event: PageEvent, cx: &mut VisualTestContext) {
    let sender = pages.events.borrow().clone().expect("a page");
    sender.unbounded_send(event).unwrap();
    cx.run_until_parked();
}

fn address(surface: &Entity<BrowserSurface>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| surface.read(cx).address.read(cx).text().to_string())
}

#[gpui::test]
fn the_page_reports_back_and_its_url_is_kept(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("http://localhost:5173/app"), &pages);
    assert_eq!(
        pages.opened_on.borrow().as_slice(),
        ["http://localhost:5173/app"]
    );
    assert_eq!(address(&surface, cx), "http://localhost:5173/app");
    assert!(cx.update(|_, cx| surface.read(cx).loading));
    send(
        &pages,
        PageEvent::Loaded("http://localhost:5173/app?token=x#y".into()),
        cx,
    );
    send(&pages, PageEvent::Title("Vite App".into()), cx);
    let (loading, title) = cx.update(|_, cx| {
        let surface = surface.read(cx);
        (surface.loading, surface.title.clone())
    });
    assert!(!loading);
    assert_eq!(title.as_deref(), Some("Vite App"));
    // The scope's last URL went to the database without its secrets.
    let saved = dcc_infra::db::SqliteSessionRepo::open(&db)
        .unwrap()
        .load_browser_location("w-1", Some("s-1"), 1)
        .unwrap();
    assert_eq!(saved.as_deref(), Some("http://localhost:5173/app"));
}

#[gpui::test]
fn reopening_restores_the_scopes_last_url(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    dcc_infra::db::SqliteSessionRepo::open(&db)
        .unwrap()
        .save_browser_location("w-1", Some("s-1"), "https://docs.rs/wry", now, now + 1_000)
        .unwrap();
    let pages = Rc::new(Pages::default());
    let (_, _) = open(cx, &db, None, &pages);
    assert_eq!(pages.opened_on.borrow().as_slice(), ["https://docs.rs/wry"]);
}

#[gpui::test]
fn the_address_bar_goes_through_the_omnibox_and_the_policy(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("https://example.com/"), &pages);
    let type_and_go = |text: &str, cx: &mut VisualTestContext| {
        let text = text.to_string();
        cx.update(|_, cx| {
            surface.update(cx, |surface, cx| {
                surface
                    .address
                    .update(cx, |input, cx| input.set_text(text, cx));
                surface.navigate(cx);
            })
        });
        cx.run_until_parked();
    };
    type_and_go("localhost:5173", cx);
    assert_eq!(
        pages.page.calls.borrow().last().map(String::as_str),
        Some("load http://localhost:5173/")
    );
    type_and_go("como testar wry", cx);
    assert_eq!(
        pages.page.calls.borrow().last().map(String::as_str),
        Some("load https://www.google.com/search?q=como%20testar%20wry")
    );
    // Plain HTTP off loopback is refused, and the strip says why.
    let loads = pages
        .page
        .calls
        .borrow()
        .iter()
        .filter(|call| call.starts_with("load"))
        .count();
    type_and_go("http://example.com", cx);
    let error = cx.update(|_, cx| surface.read(cx).error.clone());
    assert_eq!(
        error.as_deref(),
        Some("HTTP browser URLs are only allowed for localhost")
    );
    let after = pages
        .page
        .calls
        .borrow()
        .iter()
        .filter(|call| call.starts_with("load"))
        .count();
    assert_eq!(loads, after, "nothing loaded");
    assert!(cx.debug_bounds("browser-strip").is_some());
    // A load clears the error.
    send(
        &pages,
        PageEvent::Loading("https://example.com/".into()),
        cx,
    );
    assert!(cx.update(|_, cx| surface.read(cx).error.is_none()));
}

#[gpui::test]
fn the_page_shows_and_hides_under_an_overlay(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("https://example.com/"), &pages);
    let shown = |cx: &mut VisualTestContext| cx.update(|_, cx| surface.read(cx).shown);
    assert!(shown(cx), "drawn and uncovered");
    // A dialog over everything: the page hides on the next frame.
    cx.update(|window, cx| {
        let viewport = gpui::Bounds::new(gpui::point(px(0.0), px(0.0)), window.viewport_size());
        crate::ui::occlusion::mark(viewport, window, cx);
        window.refresh();
    });
    cx.run_until_parked();
    assert!(!shown(cx));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert!(shown(cx), "back once nothing covers it");
    // The splitter's drag keeps it hidden.
    cx.update(|_, cx| surface.update(cx, |surface, cx| surface.set_force_hidden(true, cx)));
    cx.run_until_parked();
    assert!(!shown(cx));
    let calls = pages.page.calls.borrow().clone();
    assert!(calls.contains(&"visible true".to_string()));
    assert!(calls.contains(&"visible false".to_string()));
}

#[gpui::test]
fn the_pages_context_goes_to_the_agent_or_says_why_not(cx: &mut TestAppContext) {
    use super::BrowserEvent;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("https://example.com/docs"), &pages);
    let sent: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = sent.clone();
    cx.update(|_, cx| {
        cx.subscribe(&surface, move |_, event: &BrowserEvent, _| {
            if let BrowserEvent::SendToAgent(text) = event {
                sink.borrow_mut().push(text.clone());
            }
        })
        .detach();
    });
    // Still loading: nothing to read yet.
    assert!(!cx.update(|_, cx| surface.read(cx).can_send()));
    send(
        &pages,
        PageEvent::Loaded("https://example.com/docs".into()),
        cx,
    );
    send(&pages, PageEvent::Title("Docs".into()), cx);
    *pages.page.answer.borrow_mut() = Some(
        r#"{"text":"Guia do <checkout>","selectionOnly":true,"truncated":false,"documentIdentity":3.5,"semanticMap":{"items":[{"role":"button","name":"Salvar","ordinal":2,"tag":"button","contentEditable":false}],"truncated":false}}"#.into(),
    );
    let button = cx
        .debug_bounds("browser-send-agent")
        .expect("the send button");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let text = sent.borrow().first().cloned().expect("sent to the agent");
    assert!(text.starts_with("<browser_page>"));
    assert!(text.contains("URL: https://example.com/docs"));
    assert!(text.contains("Título: Docs"));
    assert!(text.contains("Origem: texto selecionado"));
    assert!(text.contains("Guia do &lt;checkout>"));
    assert!(text.contains("- [e1] button | Nome: Salvar"));
    assert!(text.contains("ID do mapa: m-"));
    // A page that never answers says so after 2s.
    *pages.page.answer.borrow_mut() = None;
    cx.update(|_, cx| surface.update(cx, |surface, cx| surface.send_to_agent(cx)));
    assert!(cx.update(|_, cx| surface.read(cx).sending));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(3));
    cx.run_until_parked();
    let error = cx.update(|_, cx| surface.read(cx).error.clone());
    assert_eq!(error.as_deref(), Some("browser page context timed out"));
    assert_eq!(sent.borrow().len(), 1);
}

fn fake_sessions() -> super::sessions::SessionSource {
    use dcc_infra::browser_sessions::{
        BrowserSessionProfile, PreparedImport, PreparedSessionCookie,
    };
    let cookie = |name: &str| PreparedSessionCookie {
        name: name.into(),
        value: "v".into(),
        domain: ".example.com".into(),
        path: "/".into(),
        secure: true,
        http_only: true,
        same_site: 1,
        expires_unix_seconds: None,
    };
    super::sessions::SessionSource {
        list: Rc::new(|| {
            vec![BrowserSessionProfile {
                id: "chrome:Default".into(),
                browser: "chrome".into(),
                name: "Default".into(),
            }]
        }),
        prepare: std::sync::Arc::new(move |profile, url| {
            assert_eq!(profile, "chrome:Default");
            assert_eq!(url, "https://example.com/conta");
            Ok(PreparedImport {
                cookies: vec![cookie("sid"), cookie("csrf")],
                skipped: 1,
            })
        }),
    }
}

#[gpui::test]
fn a_chrome_session_goes_into_the_browser_and_the_page_reloads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("database.sqlite");
    let pages = Rc::new(Pages::default());
    let (surface, cx) = open(cx, &db, Some("https://example.com/conta"), &pages);
    cx.update(|_, cx| crate::ui::Toaster::init(cx));
    send(
        &pages,
        PageEvent::Loaded("https://example.com/conta".into()),
        cx,
    );
    cx.update(|_, cx| surface.update(cx, |surface, _| surface.session_source = fake_sessions()));
    let click = |cx: &mut VisualTestContext, selector: &'static str| {
        cx.run_until_parked();
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} on screen"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.run_until_parked();
    };
    click(cx, "browser-sessions");
    let site = cx.update(|_, cx| surface.read(cx).sessions.as_ref().unwrap().site());
    assert_eq!(site.as_deref(), Some("example.com"));
    assert!(
        cx.debug_bounds("browser-sessions-import").is_some(),
        "the dialog draws"
    );
    click(cx, "browser-session-profile-0");
    click(cx, "browser-sessions-import");
    assert!(
        pages
            .page
            .calls
            .borrow()
            .iter()
            .any(|call| call == "cookies 2"),
        "{:?}",
        pages.page.calls.borrow()
    );
    assert!(
        cx.update(|_, cx| surface.read(cx).sessions.is_none()),
        "closed"
    );
    assert!(
        pages
            .page
            .calls
            .borrow()
            .iter()
            .any(|call| call == "reload")
    );
    let toasts = cx.update(|_, cx| crate::ui::Toaster::messages(cx));
    assert!(
        toasts
            .iter()
            .any(|toast| toast.contains("2 sessão(ões) importada(s) · 1 ignorada(s)"))
    );
    // A page that moved on meanwhile stops the import.
    send(
        &pages,
        PageEvent::Loaded("https://example.com/conta".into()),
        cx,
    );
    click(cx, "browser-sessions");
    click(cx, "browser-session-profile-0");
    cx.update(|_, cx| surface.update(cx, |surface, _| surface.loading = true));
    click(cx, "browser-sessions-import");
    let error = cx.update(|_, cx| surface.read(cx).sessions.as_ref().unwrap().error.clone());
    assert_eq!(
        error.as_deref(),
        Some("browser changed page; reopen Sessions and try again")
    );
}
