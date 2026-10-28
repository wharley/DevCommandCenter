//! The "Pull Requests" hub in the center (`PullRequestsHub`): the open pull
//! requests across the projects on the left (filters, search, cards), the
//! chosen one on the right — open its task or "Trabalhar nisto", open it on
//! the forge — and its summary (branch, facts, description, checks,
//! activity) with a comment box. Under 780px only one side shows, with
//! "Voltar". A GitHub pull request merges from here, after a confirmation;
//! its code tab reads the patches and sends a review. The Reviewer comes
//! next.

mod code;
mod detail;
mod list;
mod merge;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{AppContext, Context, Entity, EventEmitter, IntoElement, Render, Subscription, Window};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::pr_hub::{Detail, Filter, Item, List, filtered};
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
    /// The hub's width, measured as it paints.
    pub(crate) width: Rc<Cell<f32>>,
    _inputs: Vec<Subscription>,
}

impl EventEmitter<PrHubEvent> for PrHub {}

fn message(error: ApiError, cx: &gpui::App) -> String {
    match error {
        ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
        error => error.to_string(),
    }
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
        let mut hub = Self {
            api: if cfg!(test) { None } else { DesktopApi::load() },
            list: None,
            fetching: false,
            filter: Filter::All,
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
        filtered(self.items(), self.filter, self.search.read(cx).text())
            .into_iter()
            .cloned()
            .collect()
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
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_list() })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.fetching = false;
                hub.list = Some(read.map_err(|error| message(error, cx)));
                hub.follow_selection(cx);
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
        self.ensure_detail(cx);
        cx.notify();
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
