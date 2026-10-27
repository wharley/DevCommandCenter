//! The Browser in the task's workbench (`session-workbench.tsx`): beside
//! the conversation behind a 6px splitter when both fit, over it when they
//! don't; opening it hides the terminal dock, and it closes with its task.
//! Its width is kept per task.

use gpui::{
    AnyElement, AppContext, Context, CursorStyle, EmptyView, Entity, KeyDownEvent, MouseButton,
    MouseDownEvent, Subscription, Window, div, prelude::*, px,
};

use super::Shell;
use crate::screens::browser::layout::{
    KEY_STEP, MAX_WIDTH, MIN_WIDTH, SPLITTER_WIDTH, Widths, clamp_width_for, should_split,
};
use crate::screens::browser::surface::{PageFactory, wry_pages};
use crate::screens::browser::{BrowserEvent, BrowserRequest, BrowserSurface};

/// The splitter's own drag.
#[derive(Clone)]
pub(super) struct BrowserResize;

pub(super) struct OpenBrowser {
    pub surface: Entity<BrowserSurface>,
    _events: Subscription,
    /// Pointer x and width when a splitter drag started.
    drag: Option<(f32, f32)>,
    /// The agent's open it was opened for, until the desktop confirms it.
    pub approval: Option<crate::data::browser_opens::OpenRequest>,
}

/// What the Shell keeps for the Browser.
pub(super) struct BrowserState {
    pub open: Option<OpenBrowser>,
    pub widths: Widths,
    pub width: f32,
    pub pages: PageFactory,
    /// Allowed opens waiting for their conversation to show.
    pub waiting: Vec<crate::data::browser_opens::OpenRequest>,
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            open: None,
            widths: Widths::load(&Widths::default_path()),
            width: crate::screens::browser::layout::DEFAULT_WIDTH,
            pages: wry_pages(),
            waiting: Vec::new(),
        }
    }
}

impl Shell {
    /// The room the workbench has: the window less the sidebar and code column.
    fn workbench_width(&self, window: &Window, cx: &gpui::App) -> f32 {
        let column = if self.code_column_shown(cx) {
            self.code_column.read(cx).tab.width()
        } else {
            0.0
        };
        f32::from(window.viewport_size().width) - self.layout.sidebar_rail_width() - column
    }

    /// `openBrowserSurface`: the terminal dock steps aside; a new lifecycle.
    pub(super) fn open_browser(
        &mut self,
        initial_url: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conversation = self.conversation.read(cx);
        let Some(task_id) = conversation.data().map(|data| data.task_id.clone()) else {
            return;
        };
        let session_id = conversation.shown_session();
        self.terminal.update(cx, |dock, cx| dock.hide(cx));
        self.browser.width = self.browser.widths.get(&task_id);
        let request = BrowserRequest {
            workspace_id: task_id,
            session_id,
            initial_url,
        };
        let pages = self.browser.pages.clone();
        let db_path = crate::data::loader::database_path();
        let surface = cx.new(|cx| BrowserSurface::new(request, pages, db_path, window, cx));
        let events = cx.subscribe_in(
            &surface,
            window,
            |shell, _, event, window, cx| match event {
                BrowserEvent::Closed => shell.close_browser(cx),
                BrowserEvent::SendToAgent(text) => shell.browser_to_agent(text.clone(), window, cx),
                BrowserEvent::SendEvidence(text) => {
                    shell.browser_evidence_to_agent(text.clone(), window, cx)
                }
                BrowserEvent::Ready(token) => shell.browser_ready(*token, cx),
            },
        );
        self.browser.open = Some(OpenBrowser {
            surface,
            _events: events,
            drag: None,
            approval: None,
        });
        self.sync_header(cx);
        cx.notify();
    }

    /// `handleSendBrowserToAgent`: the page's context into the draft; then
    /// the composer, beside the Browser, or the Browser closes over the chat.
    fn browser_to_agent(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.conversation.read(cx).composer.clone();
        let added = composer.update(cx, |composer, cx| composer.append_text(&text, window, cx));
        if !added {
            return;
        }
        if should_split(self.workbench_width(window, cx)) {
            self.conversation
                .update(cx, |conversation, cx| conversation.focus_composer_soon(cx));
        } else {
            self.close_browser(cx);
        }
    }

    /// `handleSendBrowserEvidenceToAgent`: the evidence into the draft; the
    /// Browser stays, and beside it the composer takes the focus.
    fn browser_evidence_to_agent(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let composer = self.conversation.read(cx).composer.clone();
        let added = composer.update(cx, |composer, cx| composer.append_text(&text, window, cx));
        if added && should_split(self.workbench_width(window, cx)) {
            self.conversation
                .update(cx, |conversation, cx| conversation.focus_composer_soon(cx));
        }
    }

    /// `handleCloseBrowser`: the composer takes the focus back.
    pub(super) fn close_browser(&mut self, cx: &mut Context<Self>) {
        if self.browser.open.take().is_some() {
            self.conversation
                .update(cx, |conversation, cx| conversation.focus_composer_soon(cx));
            self.sync_header(cx);
            cx.notify();
        }
    }

    /// Another task: its Browser does not follow.
    pub(super) fn browser_task_changed(&mut self, task_id: Option<&str>, cx: &mut Context<Self>) {
        let open_for = self
            .browser
            .open
            .as_ref()
            .map(|open| open.surface.read(cx).request.workspace_id.clone());
        if open_for.is_some() && open_for.as_deref() != task_id {
            if let Some(open) = self.browser.open.take() {
                open.surface.update(cx, |surface, cx| surface.close(cx));
            }
            self.sync_header(cx);
            cx.notify();
        }
    }

    /// Another surface took the center: the page must not float over it.
    pub(super) fn hide_browser_page(&self, cx: &mut Context<Self>) {
        if let Some(open) = &self.browser.open {
            open.surface
                .update(cx, |surface, _| surface.show_page(false));
        }
    }

    pub(super) fn browser_open(&self) -> bool {
        self.browser.open.is_some()
    }

    fn start_browser_resize(&mut self, pointer_x: f32, cx: &mut Context<Self>) {
        let width = self.browser.width;
        if let Some(open) = self.browser.open.as_mut() {
            open.drag = Some((pointer_x, width));
            open.surface
                .update(cx, |surface, cx| surface.set_force_hidden(true, cx));
        }
    }

    pub(super) fn resize_browser(
        &mut self,
        pointer_x: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let container = self.workbench_width(window, cx);
        let Some((origin, width)) = self.browser.open.as_ref().and_then(|open| open.drag) else {
            return;
        };
        // Dragging left grows it.
        let next = clamp_width_for(width + origin - pointer_x, container);
        if next != self.browser.width {
            self.browser.width = next;
            cx.notify();
        }
    }

    pub(super) fn finish_browser_resize(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.browser.open.as_mut() else {
            return;
        };
        if open.drag.take().is_none() {
            return;
        }
        let surface = open.surface.clone();
        surface.update(cx, |surface, cx| surface.set_force_hidden(false, cx));
        let task = surface.read(cx).request.workspace_id.clone();
        let width = self.browser.width;
        self.browser.widths.set(&task, width);
    }

    /// The splitter's keys: ←/→ by 16px, Home/End to the bounds.
    fn browser_key(&mut self, key: &str, window: &Window, cx: &mut Context<Self>) -> bool {
        let next = match key {
            "left" => self.browser.width + KEY_STEP,
            "right" => self.browser.width - KEY_STEP,
            "home" => MIN_WIDTH,
            "end" => MAX_WIDTH,
            _ => return false,
        };
        let container = self.workbench_width(window, cx);
        self.browser.width = clamp_width_for(next, container);
        if let Some(open) = &self.browser.open {
            let task = open.surface.read(cx).request.workspace_id.clone();
            self.browser.widths.set(&task, self.browser.width);
        }
        cx.notify();
        true
    }

    /// The conversation with the Browser beside it, or the Browser alone.
    pub(super) fn with_browser(
        &mut self,
        content: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(open) = &self.browser.open else {
            return content;
        };
        let surface = open.surface.clone();
        let container = self.workbench_width(window, cx);
        let split = should_split(container);
        let colors = crate::theme::ActiveTheme::theme(&**cx).colors.clone();
        if !split {
            // `browserTakeover`: the chat steps aside, still mounted.
            return div()
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(div().hidden().child(content))
                .child(surface)
                .into_any_element();
        }
        let width = clamp_width_for(self.browser.width, container);
        let shell = cx.entity().downgrade();
        let hover = colors.muted.opacity(0.7);
        let splitter = div()
            .id("browser-splitter")
            .debug_selector(|| "browser-splitter".into())
            .w(px(SPLITTER_WIDTH))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(colors.border.opacity(0.6))
            .bg(colors.muted.opacity(0.2))
            .hover(move |style| style.bg(hover))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x = f32::from(event.position.x);
                let _ = shell.update(cx, |shell, cx| shell.start_browser_resize(x, cx));
            })
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                if shell.browser_key(&event.keystroke.key, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_drag(BrowserResize, |_, _, window, cx| {
                cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                cx.new(|_| EmptyView)
            });
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(content),
            )
            .child(
                div()
                    .debug_selector(|| "browser-split".into())
                    .flex()
                    .flex_shrink_0()
                    .min_h_0()
                    .w(px(width + SPLITTER_WIDTH))
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.background)
                    .child(splitter)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .child(surface),
                    ),
            )
            .into_any_element()
    }
}

/// `spawn_cache_limit`: three minutes after start and then daily, the
/// Browser's caches go once they pass 500 MB; logins stay.
pub(super) fn trim_cache_daily(cx: &mut gpui::Context<Shell>) {
    use crate::screens::browser::data_store;
    if cfg!(test) || !data_store::usage().supported {
        return;
    }
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_secs(180))
            .await;
        loop {
            let before = data_store::usage().cache_bytes;
            if before > data_store::CACHE_LIMIT_BYTES {
                let trimmed = this.update(cx, |_, _| {
                    data_store::clear(false, move |result| match result {
                        Ok(()) => eprintln!(
                            "[dcc-native] Browser cache trimmed: {} MB -> {} MB",
                            before / (1024 * 1024),
                            data_store::usage().cache_bytes / (1024 * 1024)
                        ),
                        Err(error) => eprintln!("[dcc-native] Browser cache left as is: {error}"),
                    })
                });
                if trimmed.is_err() {
                    return;
                }
            }
            cx.background_executor()
                .timer(std::time::Duration::from_secs(24 * 60 * 60))
                .await;
        }
    })
    .detach();
}
