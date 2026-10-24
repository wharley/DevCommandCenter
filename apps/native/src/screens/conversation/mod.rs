//! Screen 2: a task's conversation (`ActiveThreadViewport`). Reads the
//! session the desktop opens for the task and shows its history the way the
//! desktop does — prompts, one assistant row per turn with its activity,
//! delegation cards and system notes — following new events while it is open.
//! Read-only for now: answering, approving and sending stay in the desktop.

mod activity;
mod assistant_row;
mod cards;
mod delegation_card;
mod hover_actions;
mod paced;
mod permission_panel;
mod requests;
mod system_rows;
#[cfg(test)]
mod tests;
mod tool_row;
mod user_row;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::{Duration, Instant};

use dcc_core::domain::timeline::{Role, TimelineMessage};
use gpui::{
    AnyElement, Context, Entity, ListAlignment, ListState, SharedString, Task, Window, div, list,
    prelude::*, px,
};

use crate::data::conversation::{self, Conversation, EventCache};
use crate::data::{loader, time};
use crate::i18n::{I18n, Locale, Translate};
use crate::markdown::{Block, parse};
use crate::screens::composer::{Composer, ComposerEvent, ComposerSession};
use crate::theme::ActiveTheme;
use crate::ui::ListScrollbar;

/// How often an open conversation looks for new events.
const REFRESH_LIVE: Duration = Duration::from_millis(800);
const REFRESH_IDLE: Duration = Duration::from_secs(3);
/// Mascot step and live clock while a turn runs.
const LIVE_STEP: Duration = Duration::from_millis(450);
/// The thread list starts its trail gutter at this width (`@container 54rem`).
const TRAIL_WIDTH: f32 = 864.0;

/// A streaming text's reveal, the length last laid out and its blocks.
type PacedProse = (paced::Paced, usize, Rc<Vec<Block>>);

pub struct ConversationView {
    pub(crate) composer: Entity<Composer>,
    _composer_events: gpui::Subscription,
    task_id: Option<String>,
    wanted_session: Option<String>,
    data: Option<Rc<Conversation>>,
    loading: bool,
    list: ListState,
    /// Disclosures the person opened (folds, tool details, reports…).
    open: HashSet<SharedString>,
    /// How many steps each long run shows (`RUN_PAGE_SIZE` pages).
    run_limits: HashMap<SharedString, usize>,
    markdown: RefCell<HashMap<u64, Rc<Vec<Block>>>>,
    /// Streaming prose being revealed, with what was last laid out.
    paced: RefCell<HashMap<SharedString, PacedProse>>,
    mascot_frame: bool,
    now_ms: i64,
    /// The list's width at the last frame (rows can't ask the list while it
    /// lays them out).
    viewport_width: f32,
    scrolled_at: Option<Instant>,
    _scrollbar_fade: Option<Task<()>>,
    _refresh: Option<Task<()>>,
    /// Wakes the refresh loop early (after an answer is sent).
    refresh_wake: Option<futures::channel::mpsc::UnboundedSender<()>>,
    pub(crate) requests: requests::Requests,
    _clock: Task<()>,
}

impl ConversationView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| Composer::new(window, cx));
        let composer_events =
            cx.subscribe(
                &composer,
                |view, _, event: &ComposerEvent, cx| match event {
                    ComposerEvent::Refresh => view.refresh_now(cx),
                },
            );
        let list = ListState::new(0, ListAlignment::Bottom, px(800.0));
        let weak = cx.weak_entity();
        list.set_scroll_handler(move |_, _, cx| {
            let _ = weak.update(cx, |view: &mut ConversationView, cx| view.list_scrolled(cx));
        });
        Self {
            composer,
            _composer_events: composer_events,
            task_id: None,
            wanted_session: None,
            data: None,
            loading: false,
            list,
            open: HashSet::new(),
            run_limits: HashMap::new(),
            markdown: RefCell::new(HashMap::new()),
            paced: RefCell::new(HashMap::new()),
            mascot_frame: false,
            now_ms: time::now_ms(),
            viewport_width: 0.0,
            scrolled_at: None,
            _scrollbar_fade: None,
            _refresh: None,
            refresh_wake: None,
            requests: requests::Requests::default(),
            _clock: Self::clock_loop(cx),
        }
    }

    pub fn task_id(&self) -> Option<&str> {
        self.task_id.as_deref()
    }

    /// Another conversation of the open task (a child's, from its card).
    pub(crate) fn open_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        if let Some(task_id) = self.task_id.clone() {
            self.open(&task_id, Some(session_id), cx);
        }
    }

    /// Shows `task_id`'s conversation: `session_id` when given, else the one
    /// the desktop opens first.
    pub fn open(&mut self, task_id: &str, session_id: Option<String>, cx: &mut Context<Self>) {
        let same_task = self.task_id.as_deref() == Some(task_id);
        if same_task && self.wanted_session == session_id && self.data.is_some() {
            return;
        }
        if !same_task {
            self.open.clear();
            self.run_limits.clear();
            self.markdown.borrow_mut().clear();
            self.paced.borrow_mut().clear();
        }
        self.task_id = Some(task_id.to_string());
        self.wanted_session = session_id;
        self.data = None;
        self.loading = true;
        self.list.reset(0);
        if !same_task {
            self.requests.clear();
        }
        let (wake, woken) = futures::channel::mpsc::unbounded();
        self.refresh_wake = Some(wake);
        self._refresh = Some(self.refresh_loop(woken, cx));
        cx.notify();
    }

    /// Reads again now instead of at the next beat.
    pub(crate) fn refresh_now(&self, _cx: &mut Context<Self>) {
        if let Some(wake) = &self.refresh_wake {
            let _ = wake.unbounded_send(());
        }
    }

    fn refresh_loop(
        &self,
        mut woken: futures::channel::mpsc::UnboundedReceiver<()>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let db_path = loader::database_path();
        let task_id = self.task_id.clone().unwrap_or_default();
        let wanted = self.wanted_session.clone();
        cx.spawn(async move |this, cx| {
            let mut cache = EventCache::default();
            loop {
                let (path, task, session) = (db_path.clone(), task_id.clone(), wanted.clone());
                let (loaded, kept) = cx
                    .background_executor()
                    .spawn(async move {
                        let loaded =
                            conversation::load(&path, &task, session.as_deref(), &mut cache);
                        (loaded, cache)
                    })
                    .await;
                cache = kept;
                let next = match loaded {
                    Ok(data) => Rc::new(data),
                    Err(error) => {
                        eprintln!("[dcc-native] conversation: {error:#}");
                        cx.background_executor().timer(REFRESH_IDLE).await;
                        continue;
                    }
                };
                let running = next.running;
                if this.update(cx, |view, cx| view.apply(next, cx)).is_err() {
                    return;
                }
                let beat = cx.background_executor().timer(if running {
                    REFRESH_LIVE
                } else {
                    REFRESH_IDLE
                });
                futures::future::select(Box::pin(beat), futures::StreamExt::next(&mut woken)).await;
            }
        })
    }

    fn apply(&mut self, next: Rc<Conversation>, cx: &mut Context<Self>) {
        if self.task_id.as_deref() != Some(next.task_id.as_str()) {
            return;
        }
        let was_loading = self.loading;
        self.loading = false;
        if let Some(current) = &self.data
            && current.same_as(&next)
        {
            if was_loading {
                cx.notify();
            }
            return;
        }
        let old_rows: &[TimelineMessage] = self.data.as_ref().map_or(&[], |data| &data.rows);
        let (span, count) = changed_span(old_rows, &next.rows);
        if self.data.is_none() {
            self.list.reset(next.rows.len() + 2);
        } else if span.start != span.end || count > 0 {
            // Items are [top spacer, rows…, bottom spacer].
            self.list.splice(span.start + 1..span.end + 1, count);
        }
        let session = ComposerSession {
            task_id: Some(next.task_id.clone()),
            session_id: next.session_id.clone(),
            provider_id: next.provider_id.clone(),
            model: next.model.clone(),
            running: next.running,
            aborted: next.aborted,
            workspace_path: next.workspace_path.clone(),
            workspace_name: next.workspace_name.clone(),
            branch: next.branch.clone(),
            usage_limit: next.usage_limit.clone(),
            goal: next.goal.clone(),
            objective: next.objective.clone(),
            auto_resume: next.auto_resume,
            context: next.context.clone(),
        };
        let history = next.rows.clone();
        self.composer.update(cx, |composer, cx| {
            composer.set_history(history);
            composer.set_session(session, cx)
        });
        self.data = Some(next);
        cx.notify();
    }

    /// "Tentar de novo" on the interrupted last turn.
    pub(crate) fn retry(
        &mut self,
        prompt: String,
        turn_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.retry(prompt, turn_id, window, cx)
        });
    }

    /// "Continuar": the continuation prompt waits in the composer.
    pub(crate) fn continue_interrupted(
        &mut self,
        original: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            composer.continue_interrupted(original, window, cx)
        });
    }

    /// Mascot steps and the live clock while a turn runs; otherwise the
    /// "há 5 minutos" stamps only need a beat every 30 seconds.
    fn clock_loop(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LIVE_STEP).await;
                let alive = this.update(cx, |view, cx| {
                    let now = time::now_ms();
                    let live = view.data.as_ref().is_some_and(|data| data.running);
                    if !live && now - view.now_ms < 30_000 {
                        return;
                    }
                    view.now_ms = now;
                    view.mascot_frame = live && !view.mascot_frame;
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        })
    }

    fn list_scrolled(&mut self, cx: &mut Context<Self>) {
        self.scrolled_at = Some(Instant::now());
        self._scrollbar_fade = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::ui::scrollbar::SCROLLBAR_LINGER + Duration::from_millis(50))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        }));
        cx.notify();
    }

    pub(crate) fn is_open(&self, key: &str) -> bool {
        self.open.contains(key)
    }

    pub(crate) fn toggle(&mut self, key: SharedString, cx: &mut Context<Self>) {
        if !self.open.remove(&key) {
            self.open.insert(key);
        }
        cx.notify();
    }

    pub(crate) fn run_limit(&self, key: &str, page: usize) -> usize {
        self.run_limits.get(key).copied().unwrap_or(page)
    }

    pub(crate) fn show_earlier(&mut self, key: SharedString, page: usize, cx: &mut Context<Self>) {
        let limit = self.run_limit(&key, page) + page;
        self.run_limits.insert(key, limit);
        cx.notify();
    }

    /// Parsed once per distinct text; rows re-render every frame they show.
    pub(crate) fn markdown(&self, text: &str) -> Rc<Vec<Block>> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        self.markdown
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| Rc::new(parse(text)))
            .clone()
    }

    /// Streaming prose as far as the reveal got, with the caret, asking for
    /// the next frame while there is more to show.
    pub(crate) fn paced_markdown(
        &self,
        key: &str,
        target: &str,
        window: &mut Window,
    ) -> Rc<Vec<Block>> {
        let mut paced = self.paced.borrow_mut();
        let (state, laid_out, blocks) = paced
            .entry(SharedString::from(key.to_string()))
            .or_insert_with(|| (paced::Paced::new(target), usize::MAX, Rc::new(Vec::new())));
        if state.advance(target, Instant::now()) {
            window.request_animation_frame();
        }
        if *laid_out != state.shown {
            *laid_out = state.shown;
            *blocks = Rc::new(parse(&format!(
                "{}{}",
                &target[..state.shown],
                paced::CARET
            )));
        }
        blocks.clone()
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.now_ms
    }

    pub(crate) fn mascot_frame(&self) -> bool {
        self.mascot_frame
    }

    fn render_item(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = self.data.clone() else {
            return div().into_any_element();
        };
        let rows = &data.rows;
        if index == 0 {
            return div().h(px(24.0)).into_any_element();
        }
        let Some(message) = rows.get(index - 1) else {
            return div().h(px(40.0)).into_any_element();
        };
        let viewport = self.viewport_width;
        let content = row_element(self, &data, message, window, cx);
        div()
            .w_full()
            .min_w_0()
            .px(px(20.0))
            .when(viewport >= TRAIL_WIDTH, |row| row.pl(px(72.0)))
            .pb(px(16.0))
            .child(content)
            .into_any_element()
    }
}

/// The span to replace so `old` becomes `new`, keeping unchanged rows at
/// both ends measured and the reading position where it is.
fn changed_span(
    old: &[TimelineMessage],
    new: &[TimelineMessage],
) -> (std::ops::Range<usize>, usize) {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    (prefix..old.len() - suffix, new.len() - prefix - suffix)
}

fn row_element(
    view: &ConversationView,
    data: &Conversation,
    message: &TimelineMessage,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    match message.role {
        Role::User => match &message.hand_back {
            Some(hand_back) => system_rows::hand_back_row(view, message, hand_back, cx),
            None => user_row::user_row(view, message, window, cx),
        },
        Role::Assistant => assistant_row::assistant_row(view, data, message, window, cx),
        Role::System => match &message.delegation {
            Some(delegation) => {
                delegation_card::delegation_card(view, data, message, delegation, window, cx)
            }
            None => system_rows::system_row(view, message, cx),
        },
    }
}

/// "há 5 minutos" for a row's stamp, in the app's language.
pub(crate) fn time_ago(
    view: &ConversationView,
    stamp: Option<&str>,
    cx: &gpui::App,
) -> Option<SharedString> {
    let at = time::parse_ms(stamp?)?;
    let english = cx.global::<I18n>().locale() == Locale::En;
    Some(time::distance_to_now(at, view.now_ms(), english).into())
}

impl Render for ConversationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport_width = f32::from(self.list.viewport_bounds().size.width);
        let muted = cx.theme().colors.muted_foreground;
        let entity = cx.entity();
        let empty = self.data.as_ref().is_none_or(|data| data.rows.is_empty());
        let body = if self.loading || empty {
            let note = if self.loading {
                Some(cx.t("conversation.loading.title"))
            } else {
                None
            };
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(13.0))
                .text_color(muted)
                .children(note)
                .into_any_element()
        } else {
            list(self.list.clone(), move |index, window, cx| {
                entity.update(cx, |view, cx| view.render_item(index, window, cx))
            })
            .size_full()
            .into_any_element()
        };
        div()
            .id("conversation")
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .size_full()
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(body)
                    .when(!self.loading && !empty, |area| {
                        area.child(ListScrollbar::new(
                            "conversation-scrollbar",
                            self.list.clone(),
                            self.scrolled_at,
                        ))
                    }),
            )
            .children(permission_panel::permission_panel(self, cx))
            .when(self.task_id.is_some(), |area| {
                area.child(self.composer.clone())
            })
    }
}
