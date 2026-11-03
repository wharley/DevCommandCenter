//! The "Pull Requests" hub in the center (`PullRequestsHub`): the open pull
//! requests across the projects on the left (filters, search, cards), the
//! chosen one on the right — open its task or "Trabalhar nisto", open it on
//! the forge — and its summary (branch, facts, description, checks,
//! activity) with a comment box. Under 780px only one side shows, with
//! "Voltar". A GitHub pull request merges from here, after a confirmation;
//! its code tab reads the patches and sends a review, with the built-in
//! reviewer's help when the person asks for it.

mod code;
mod detail;
mod list;
mod merge;
#[cfg(test)]
mod news_tests;
mod reviewer;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AppContext, BorrowAppContext, Context, Entity, EventEmitter, IntoElement, Render, Subscription,
    Window,
};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::pr_hub::{Detail, Filter, Item, List, filtered};
use crate::data::pr_news::PrNews;
use crate::i18n::Translate;
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::{TextArea, TextAreaEvent, Toaster};

/// `staleTime: 20_000` for a pull request's detail.
const DETAIL_FRESH: Duration = Duration::from_secs(20);
/// `dcc-pr-hub`'s container query.
const NARROW: f32 = 780.0;

/// The selected one's detail: its id, when it was read, and what came.
pub(crate) type DetailRead = (String, Instant, Option<Result<Detail, String>>);

pub enum PrHubEvent {
    /// "Abrir tarefa".
    OpenTask(String),
    /// "Trabalhar nisto" made a task: it opens with the composer in focus.
    Created(String),
}

pub struct PrHub {
    pub(crate) api: Option<DesktopApi>,
    /// `None` while the first read runs.
    pub(crate) list: Option<Result<List, String>>,
    pub(crate) fetching: bool,
    pub(crate) filter: Filter,
    pub(crate) search: Entity<TextInput>,
    pub(crate) selected: Option<String>,
    /// Narrow: the detail in front of the list.
    pub(crate) detail_open: bool,
    pub(crate) detail: Option<DetailRead>,
    pub(crate) comment: Entity<TextArea>,
    pub(crate) sending: bool,
    pub(crate) comment_error: Option<String>,
    pub(crate) working_on: Option<String>,
    /// The merge method picked, waiting for its confirmation.
    pub(crate) pending_merge: Option<&'static str>,
    pub(crate) merging: bool,
    pub(crate) merge_error: Option<String>,
    pub(crate) tab: code::Tab,
    /// The code tab's read, with the patches.
    pub(crate) code_detail: Option<DetailRead>,
    pub(crate) review: code::CodeReview,
    /// The reviewer, the providers and the composer's pick, from the shell.
    pub(crate) sources: reviewer::ReviewerSources,
    /// What the reviewer's panel picked to run reviews on.
    pub(crate) review_pick: crate::data::pr_review_pick::ReviewPick,
    /// The accounts picked for new conversations, read as the panel opens.
    pub(crate) account_picks: crate::data::account_selection::Selection,
    /// A pull request asked to open, waiting for the list.
    pub(crate) requested: Option<String>,
    /// The pull requests with news when the hub opened, by URL: they come
    /// first, and stay where they are once opened (seen) until it opens again.
    news_order: Vec<String>,
    /// The next search sets `news_order` (the hub just opened).
    reorder: bool,
    /// The hub's width, measured as it paints.
    pub(crate) width: Rc<Cell<f32>>,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<PrHubEvent> for PrHub {}

fn message(error: ApiError, cx: &gpui::App) -> String {
    error.text(cx)
}

impl PrHub {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = cx.t("pullRequests.search");
        let search = cx.new(|cx| TextInput::new(window, cx).with_placeholder(placeholder));
        let comment_placeholder = cx.t("pullRequests.commentPlaceholder");
        let comment = cx.new(|cx| TextArea::new(window, cx).with_placeholder(comment_placeholder));
        let (review, review_inputs) = code::CodeReview::new(window, cx);
        let mut inputs = vec![
            cx.subscribe(&search, |hub, _, event, cx| {
                if matches!(event, TextInputEvent::Changed) {
                    hub.follow_selection(cx);
                    cx.notify();
                }
            }),
            cx.subscribe(&comment, |_, area, event, cx| match event {
                // A comment runs over lines: Enter is a new one.
                TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
                TextAreaEvent::Changed => cx.notify(),
                _ => {}
            }),
        ];
        inputs.extend(review_inputs);
        inputs.push(
            cx.observe_global::<crate::data::pr_review_jobs::PrReviewJobs>(|hub, cx| {
                hub.apply_review_job(cx);
                cx.notify();
            }),
        );
        inputs.push(cx.observe_global::<PrNews>(|_, cx| cx.notify()));
        let mut hub = Self {
            api: if cfg!(test) { None } else { DesktopApi::load() },
            list: None,
            fetching: false,
            filter: Filter::default(),
            search,
            selected: None,
            detail_open: false,
            detail: None,
            comment,
            sending: false,
            comment_error: None,
            working_on: None,
            pending_merge: None,
            merging: false,
            merge_error: None,
            tab: code::Tab::Summary,
            code_detail: None,
            review,
            sources: reviewer::ReviewerSources::default(),
            review_pick: crate::data::pr_review_pick::load(),
            account_picks: crate::data::account_selection::load(),
            requested: None,
            news_order: Vec::new(),
            reorder: true,
            width: Rc::new(Cell::new(1200.0)),
            _inputs: inputs,
        };
        hub.refresh(cx);
        hub
    }

    pub(crate) fn narrow(&self) -> bool {
        self.width.get() < NARROW
    }

    pub(crate) fn items(&self) -> &[Item] {
        match &self.list {
            Some(Ok(list)) => &list.items,
            _ => &[],
        }
    }

    pub(crate) fn visible(&self, cx: &gpui::App) -> Vec<Item> {
        let items = filtered(
            self.items(),
            self.filter,
            self.search.read(cx).text(),
            |item| crate::data::pr_news::reviewed(item, cx),
        );
        let first = |item: &&Item| self.news_order.contains(&item.url);
        let (mut news, rest): (Vec<&Item>, Vec<&Item>) = items.into_iter().partition(first);
        news.extend(rest);
        news.into_iter().cloned().collect()
    }

    pub(crate) fn selected_item(&self, cx: &gpui::App) -> Option<Item> {
        let id = self.selected.as_deref()?;
        self.visible(cx).into_iter().find(|item| item.id == id)
    }

    /// The first visible one when the chosen one is no longer listed.
    fn follow_selection(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible(cx);
        let kept = self
            .selected
            .as_deref()
            .is_some_and(|id| visible.iter().any(|item| item.id == id));
        if !kept {
            self.selected = visible.first().map(|item| item.id.clone());
        }
        self.ensure_detail(cx);
    }

    /// The hub opened from the sidebar: read again, news first.
    pub fn reopen(&mut self, cx: &mut Context<Self>) {
        self.reorder = true;
        self.refresh(cx);
    }

    /// `listQuery.refetch()`
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.api.clone() else {
            return;
        };
        if self.fetching {
            return;
        }
        self.fetching = true;
        cx.notify();
        self.read_reviewed(api.clone(), cx);
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_list() })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.fetching = false;
                hub.list = Some(read.map_err(|error| message(error, cx)));
                hub.follow_selection(cx);
                hub.follow_request(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.follow_selection(cx);
        cx.notify();
    }

    pub(crate) fn select(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.detail_open = true;
        self.see_selected(cx);
        self.ensure_detail(cx);
        cx.notify();
    }

    /// The pull request the person opened: what came since the review is
    /// seen. Not the one the hub shows by itself, whose tag must stay.
    fn see_selected(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        if crate::data::pr_news::is_new(&item, cx) {
            cx.update_global::<PrNews, _>(|news, _| {
                news.mark_seen(&item.url);
            });
        }
    }

    /// The pull requests the person reviewed, read with the list (one search
    /// for every repository): the hub on screen counts its news and
    /// announces none.
    fn read_reviewed(&mut self, api: DesktopApi, cx: &mut Context<Self>) {
        if !cx.has_global::<PrNews>() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_reviewed() })
                .await;
            if let Ok(list) = read {
                let _ = this.update(cx, |hub, cx| {
                    let order = cx.update_global::<PrNews, _>(|news, _| {
                        news.read(list.items);
                        news.news_urls()
                    });
                    if std::mem::take(&mut hub.reorder) {
                        hub.news_order = order;
                        hub.follow_selection(cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The selected one's detail, read again after 20s.
    pub(crate) fn ensure_detail(&mut self, cx: &mut Context<Self>) {
        self.sync_review(cx);
        self.ensure_code_detail(cx);
        let Some(item) = self.selected_item(cx) else {
            return;
        };
        let fresh = self
            .detail
            .as_ref()
            .is_some_and(|(id, at, _)| *id == item.id && at.elapsed() < DETAIL_FRESH);
        if fresh {
            return;
        }
        let Some(api) = self.api.clone() else {
            return;
        };
        let id = item.id.clone();
        self.detail = Some((id.clone(), Instant::now(), None));
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_detail(&item, false) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                if let Some((current, _, result)) = hub.detail.as_mut()
                    && *current == id
                {
                    *result = Some(read.map_err(|error| message(error, cx)));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn send_comment(&mut self, cx: &mut Context<Self>) {
        let body = self.comment.read(cx).text().to_string();
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if body.trim().is_empty() || self.sending {
            return;
        }
        self.sending = true;
        self.comment_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let sent = cx
                .background_executor()
                .spawn(async move { api.pr_hub_comment(&item, &body) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.sending = false;
                match sent {
                    Ok(()) => {
                        hub.comment.update(cx, |area, cx| area.set_text("", cx));
                        hub.detail = None;
                        hub.ensure_detail(cx);
                        hub.refresh(cx);
                    }
                    Err(error) => hub.comment_error = Some(message(error, cx)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `handleWorkOnPullRequest`: a task from the pull request's URL.
    pub(crate) fn work_on(&mut self, cx: &mut Context<Self>) {
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if self.working_on.is_some() {
            return;
        }
        self.working_on = Some(item.id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let created = cx
                .background_executor()
                .spawn(async move {
                    api.create_from_source_url(
                        &item.project_id,
                        &item.repository_root,
                        &item.url,
                        None,
                        item.forge_login.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.working_on = None;
                match created {
                    Ok(task) => {
                        cx.emit(PrHubEvent::Created(task.workspace_id));
                        hub.refresh(cx);
                    }
                    Err(error) => Toaster::error(message(error, cx), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for PrHub {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_hub(window, cx)
    }
}
