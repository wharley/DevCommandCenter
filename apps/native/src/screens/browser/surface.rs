//! The Browser of a task's conversation (`WorkspaceBrowserSurface`): it
//! opens on the URL asked for, else the scope's last one (kept 30 days,
//! without query, fragment or credentials), else Google; the address bar
//! goes through the omnibox and the URL policy; what the page reports —
//! loading, its URL, its title — shows above it.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use dcc_core::domain::browser::{
    LOCATION_TTL_MS, sanitize_location_url, select_open_url, validate_browser_url,
};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, Window};

use super::address;
use super::page::{Page, PageEvent, WryPage};
use crate::ui::text_input::{TextInput, TextInputEvent};

/// How often a single-page app's URL is read again (it moves without a load).
const URL_WATCH: Duration = Duration::from_secs(1);

/// Builds the page in a window (the real one, or a fake in tests).
pub type PageFactory =
    Rc<dyn Fn(&Window, &str, UnboundedSender<PageEvent>) -> Result<Rc<dyn Page>, String>>;

pub fn wry_pages() -> PageFactory {
    Rc::new(|window, url, events| {
        WryPage::new(window, url, events).map(|page| Rc::new(page) as Rc<dyn Page>)
    })
}

/// What a Browser opens for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserRequest {
    pub workspace_id: String,
    pub session_id: Option<String>,
    /// An address to open (a local server found in a terminal), else the last.
    pub initial_url: Option<String>,
}

pub enum BrowserEvent {
    Closed,
    /// "Enviar contexto da página ao agente": the envelope for the composer.
    SendToAgent(String),
}

/// Each Browser opened is a lifecycle of its own (the map ids say which).
static LIFECYCLES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

pub struct BrowserSurface {
    pub request: BrowserRequest,
    pub(super) address: Entity<TextInput>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    /// The URL the page opens on, until it exists.
    pending: Option<String>,
    pub(super) page: Option<Rc<dyn Page>>,
    make_page: PageFactory,
    events: Option<UnboundedSender<PageEvent>>,
    /// Whether the page shows now (it hides under overlays and drags).
    pub(super) shown: bool,
    /// The splitter is being dragged: the page stays out of the way.
    pub(super) force_hidden: bool,
    /// The page's context is being read for the agent.
    pub sending: bool,
    /// Pages finished loading, to tell a reply from an old page.
    pub(super) page_loads: u64,
    pub(super) lifecycle: u64,
    pub(super) map_generation: u64,
    pub(super) _context: Option<Task<()>>,
    /// "Sessões", while open.
    pub sessions: Option<super::sessions::SessionsDialog>,
    pub session_source: super::sessions::SessionSource,
    pub(super) session_openings: u64,
    db_path: PathBuf,
    _address_events: Subscription,
    _page_events: Option<Task<()>>,
    _url_watch: Option<Task<()>>,
    _restore: Option<Task<()>>,
}

impl EventEmitter<BrowserEvent> for BrowserSurface {}

impl BrowserSurface {
    pub fn new(
        request: BrowserRequest,
        make_page: PageFactory,
        db_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = crate::i18n::Translate::t(&**cx, "browser.addressPlaceholder");
        let address = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let address_events = cx.subscribe(&address, |surface, _, event, cx| {
            if *event == TextInputEvent::Submitted {
                surface.navigate(cx);
            }
        });
        let mut surface = Self {
            request,
            address,
            url: None,
            title: None,
            loading: true,
            error: None,
            pending: None,
            page: None,
            make_page,
            events: None,
            shown: false,
            force_hidden: false,
            sending: false,
            page_loads: 0,
            lifecycle: LIFECYCLES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
            map_generation: 0,
            _context: None,
            sessions: None,
            session_source: super::sessions::SessionSource::real(),
            session_openings: 0,
            db_path,
            _address_events: address_events,
            _page_events: None,
            _url_watch: None,
            _restore: None,
        };
        surface.choose_url(cx);
        surface
    }

    /// `select_browser_open_url`: what was asked, else (restoring) the
    /// scope's last URL, else the default.
    fn choose_url(&mut self, cx: &mut Context<Self>) {
        let request = self.request.clone();
        let db_path = self.db_path.clone();
        self._restore = Some(cx.spawn(async move |this, cx| {
            let stored = if request.initial_url.is_none() {
                cx.background_executor()
                    .spawn(async move {
                        dcc_infra::db::SqliteSessionRepo::open(&db_path)
                            .ok()?
                            .load_browser_location(
                                &request.workspace_id,
                                request.session_id.as_deref(),
                                now_ms(),
                            )
                            .ok()
                            .flatten()
                    })
                    .await
            } else {
                None
            };
            let _ = this.update(cx, |surface, cx| {
                match select_open_url(
                    surface.request.initial_url.as_deref(),
                    None,
                    stored,
                    surface.request.initial_url.is_none(),
                ) {
                    Ok(url) => {
                        let url = url.to_string();
                        surface.set_address(&url, cx);
                        surface.pending = Some(url);
                    }
                    Err(error) => {
                        surface.loading = false;
                        surface.error = Some(error);
                    }
                }
                cx.notify();
            });
        }));
    }

    /// The page, made the first time the surface is drawn in a window.
    pub(super) fn ensure_page(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.page.is_some() {
            return;
        }
        let Some(url) = self.pending.take() else {
            return;
        };
        let (sender, mut receiver) = unbounded::<PageEvent>();
        match (self.make_page)(window, &url, sender.clone()) {
            Ok(page) => {
                self.page = Some(page);
                self.events = Some(sender);
                self._page_events = Some(cx.spawn(async move |this, cx| {
                    while let Some(event) = receiver.next().await {
                        if this
                            .update(cx, |surface, cx| surface.on_page_event(event, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
                self.watch_url(cx);
            }
            Err(error) => {
                self.loading = false;
                self.error = Some(error);
            }
        }
    }

    /// A single-page app's address moves without a load.
    fn watch_url(&mut self, cx: &mut Context<Self>) {
        self._url_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(URL_WATCH).await;
                let alive = this.update(cx, |surface, cx| {
                    let Some(url) = surface.page.as_ref().and_then(|page| page.url()) else {
                        return;
                    };
                    if !surface.loading
                        && surface.url.as_deref() != Some(url.as_str())
                        && validate_browser_url(&url).is_ok()
                    {
                        surface.url = Some(url.clone());
                        surface.set_address(&url, cx);
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
    }

    pub(super) fn on_page_event(&mut self, event: PageEvent, cx: &mut Context<Self>) {
        match event {
            PageEvent::Loading(url) => {
                self.loading = true;
                self.error = None;
                self.set_address(&url, cx);
                self.url = Some(url);
            }
            PageEvent::Loaded(url) => {
                self.loading = false;
                self.page_loads += 1;
                self.set_address(&url, cx);
                self.persist(&url, cx);
                self.url = Some(url);
            }
            PageEvent::Title(title) => {
                self.title = (!title.trim().is_empty()).then_some(title);
            }
        }
        cx.notify();
    }

    /// `persist_browser_location`: the scope's last URL, sanitized.
    fn persist(&self, url: &str, cx: &mut Context<Self>) {
        let Some(safe) = sanitize_location_url(url) else {
            return;
        };
        let request = self.request.clone();
        let db_path = self.db_path.clone();
        cx.background_executor()
            .spawn(async move {
                let saved = now_ms();
                let result = dcc_infra::db::SqliteSessionRepo::open(&db_path).and_then(|repo| {
                    repo.save_browser_location(
                        &request.workspace_id,
                        request.session_id.as_deref(),
                        &safe,
                        saved,
                        saved + LOCATION_TTL_MS,
                    )
                });
                if let Err(error) = result {
                    eprintln!("[dcc-native] browser location: {error}");
                }
            })
            .detach();
    }

    fn set_address(&self, url: &str, cx: &mut Context<Self>) {
        self.address
            .update(cx, |input, cx| input.set_text(url.to_string(), cx));
    }

    /// The address bar's Enter (`handleNavigate`).
    pub fn navigate(&mut self, cx: &mut Context<Self>) {
        let typed = self.address.read(cx).text().to_string();
        let url = address::resolve(&typed);
        if url.is_empty() {
            return;
        }
        self.set_address(&url, cx);
        match validate_browser_url(&url) {
            Ok(url) => {
                self.error = None;
                self.loading = true;
                match &self.page {
                    Some(page) => page.load_url(url.as_str()),
                    None => self.pending = Some(url.to_string()),
                }
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = &self.page {
            self.loading = true;
            self.error = None;
            page.reload();
            cx.notify();
        }
    }

    /// The splitter's drag keeps the page out of the way (`forceOccluded`).
    pub fn set_force_hidden(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if self.force_hidden != hidden {
            self.force_hidden = hidden;
            cx.notify();
        }
    }

    /// Shows or hides the page, telling it only when that changes.
    pub fn show_page(&mut self, visible: bool) {
        if self.shown == visible {
            return;
        }
        self.shown = visible;
        if let Some(page) = &self.page {
            page.set_visible(visible);
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.show_page(false);
        crate::ui::occlusion::page_closed(cx);
        cx.emit(BrowserEvent::Closed);
    }
}

impl Drop for BrowserSurface {
    fn drop(&mut self) {
        if let Some(page) = &self.page {
            page.blur();
            page.set_visible(false);
        }
    }
}
