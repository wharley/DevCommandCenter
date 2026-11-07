//! A task an agent proposed (`dcc_propose_task`, or `dcc_open_task` from a
//! resident agent's own conversation), waiting in the conversation for the
//! person: its title, where it will run, the brief — which they can edit —
//! and "Começar" / "Descartar". Nothing runs until they start it.

use std::collections::HashMap;

use gpui::{
    AnyElement, Context, Entity, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::ConversationView;
use super::system_rows::centered;
use crate::data::delegation_record::DelegationRecord;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::project::{model_label, provider_label};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::text_area::TextArea;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, spinner};

/// Briefs longer than this open collapsed.
const COLLAPSED_BRIEF_CHARS: usize = 360;

/// The cards' own state: briefs being edited, the one starting, failures.
#[derive(Default)]
pub(super) struct ProposalCards {
    pub(super) editing: HashMap<String, Entity<TextArea>>,
    busy: Option<String>,
    errors: HashMap<String, SharedString>,
}

impl ConversationView {
    fn proposal_api(&self, cx: &gpui::App) -> Option<DesktopApi> {
        self.composer
            .read(cx)
            .endpoint
            .clone()
            .or_else(DesktopApi::load)
    }

    fn edit_proposal(
        &mut self,
        id: String,
        brief: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let area = cx.new(|cx| TextArea::new(window, cx).with_max_height(260.0));
        area.update(cx, |area, cx| area.set_text(&brief, cx));
        let handle = area.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        self.proposals.editing.insert(id, area);
        cx.notify();
    }

    fn answer_proposal(&mut self, id: String, start: bool, cx: &mut Context<Self>) {
        if self.proposals.busy.is_some() {
            return;
        }
        let brief = start
            .then(|| self.proposals.editing.get(&id))
            .flatten()
            .map(|area| area.read(cx).text().trim().to_string())
            .filter(|brief| !brief.is_empty());
        let api = self.proposal_api(cx);
        let answered = cx.t("delegation.proposal.answered");
        self.proposals.busy = Some(id.clone());
        self.proposals.errors.remove(&id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let id = id.clone();
                    async move {
                        let api = api.ok_or(ApiError::DesktopClosed)?;
                        if start {
                            api.start_task_proposal(&id, brief.as_deref()).map(|_| ())
                        } else {
                            api.dismiss_task_proposal(&id)
                        }
                    }
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.proposals.busy = None;
                match result {
                    Ok(()) => {
                        view.proposals.editing.remove(&id);
                    }
                    Err(error) => {
                        let text = match &error {
                            ApiError::AlreadyAnswered => answered.clone(),
                            other => SharedString::from(other.to_string()),
                        };
                        view.proposals.errors.insert(id.clone(), text);
                    }
                }
                view.refresh_now(cx);
                cx.notify();
            });
        })
        .detach();
    }
}

/// Where the task will run, who runs it and with what effort.
pub(super) fn meta_line(record: &DelegationRecord, cx: &gpui::App) -> String {
    let place = match &record.new_task {
        Some(target) => vec![
            target.project.clone(),
            if target.worktree {
                cx.t("delegation.proposal.worktree").to_string()
            } else {
                cx.t("delegation.proposal.projectCheckout").to_string()
            },
        ],
        None if !record.implement => vec![cx.t("delegation.proposal.readOnly").to_string()],
        None if record.shared_checkout => {
            vec![cx.t("delegation.proposal.sharedCheckout").to_string()]
        }
        None => vec![cx.t("delegation.proposal.worktree").to_string()],
    };
    let agent = match &record.model_id {
        Some(model) => model_label(model),
        None => provider_label(&record.provider_id)
            .map_or_else(|| record.provider_id.clone(), str::to_string),
    };
    let effort = record
        .new_task
        .as_ref()
        .and_then(|target| target.effort.as_deref())
        .map(|effort| {
            let level = cx.t(&format!("composer.effort.{effort}")).to_lowercase();
            cx.t_with(
                "delegation.proposal.effort",
                &Args::default().with("level", level),
            )
            .to_string()
        });
    place
        .into_iter()
        .chain([agent])
        .chain(effort)
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn proposal_card(
    view: &ConversationView,
    record: &DelegationRecord,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let id = record.id.clone();
    let busy = view.proposals.busy.as_deref() == Some(id.as_str());
    let locked = view.proposals.busy.is_some();
    let editing = view.proposals.editing.get(&id).cloned();
    let title = record
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| cx.t("delegation.proposal.label").to_string());
    let brief = record.instruction.clone();
    let brief_key = SharedString::from(format!("proposal-brief-{id}"));
    let brief_open = view.is_open(&brief_key);
    let long = brief.chars().count() > COLLAPSED_BRIEF_CHARS;
    let brief_view = match editing {
        Some(area) => div()
            .debug_selector(|| "proposal-brief-input".into())
            .mt(px(8.0))
            .rounded(radius.md)
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .px(px(10.0))
            .py(px(8.0))
            .min_h(px(96.0))
            .text_size(px(12.5))
            .child(area)
            .into_any_element(),
        None => {
            let blocks = view.markdown(&brief);
            let style = MarkdownStyle {
                size: 12.5,
                ..MarkdownStyle::reply()
            };
            let toggle = brief_key.clone();
            let (muted, foreground) = (colors.muted_foreground, colors.foreground);
            div()
                .mt(px(8.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(colors.foreground)
                        .when(long && !brief_open, |body| {
                            body.max_h(px(120.0)).overflow_hidden()
                        })
                        .child(render_markdown(
                            &blocks,
                            style,
                            &format!("proposal-{id}"),
                            window,
                            cx,
                        )),
                )
                .when(long, |body| {
                    body.child(
                        div()
                            .id(brief_key.clone())
                            .mt(px(4.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(muted)
                            .hover(move |style| style.text_color(foreground))
                            .on_click(
                                cx.listener(move |view, _, _, cx| view.toggle(toggle.clone(), cx)),
                            )
                            .child(Icon::new("chevron-down").size(14.0).rotate(if brief_open {
                                180.0
                            } else {
                                0.0
                            }))
                            .child(if brief_open {
                                cx.t("delegation.card.hideReport")
                            } else {
                                cx.t("delegation.card.showReport")
                            }),
                    )
                })
                .into_any_element()
        }
    };
    let hint = if record.new_task.is_some() {
        cx.t("delegation.proposal.hintNewTask")
    } else {
        cx.t("delegation.proposal.hint")
    };
    let error = view.proposals.errors.get(&id).cloned();
    let editing_now = view.proposals.editing.contains_key(&id);
    let start = {
        let id = id.clone();
        Button::new(SharedString::from(format!("proposal-start-{id}")))
            .variant(ButtonVariant::Default)
            .size(ButtonSize::Sm)
            .disabled(locked)
            .on_click(cx.listener(move |view, _, _, cx| view.answer_proposal(id.clone(), true, cx)))
            .child(if busy {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(spinner("proposal-starting", 12.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .child(cx.t("delegation.proposal.starting")),
                    )
                    .into_any_element()
            } else {
                div()
                    .text_size(px(11.5))
                    .child(cx.t("delegation.proposal.start"))
                    .into_any_element()
            })
    };
    let edit = (!editing_now).then(|| {
        let (id, brief) = (id.clone(), brief.clone());
        Button::new(SharedString::from(format!("proposal-edit-{id}")))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .disabled(locked)
            .on_click(cx.listener(move |view, _, window, cx| {
                view.edit_proposal(id.clone(), brief.clone(), window, cx)
            }))
            .child(
                div()
                    .text_size(px(11.5))
                    .child(cx.t("delegation.proposal.editBrief")),
            )
    });
    let dismiss = {
        let id = id.clone();
        Button::new(SharedString::from(format!("proposal-dismiss-{id}")))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .disabled(locked)
            .text_color(colors.muted_foreground)
            .on_click(
                cx.listener(move |view, _, _, cx| view.answer_proposal(id.clone(), false, cx)),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .child(cx.t("delegation.proposal.dismiss")),
            )
    };
    centered(
        div()
            .debug_selector(|| "proposal-card".into())
            .w_full()
            .max_w(px(672.0))
            .rounded(radius.xl)
            .border_1()
            .border_color(tw::SKY_500.alpha(0.35))
            .bg(colors.card.opacity(0.7))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(10.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(Icon::new("list-plus").size(13.0))
                    .child(cx.t("delegation.proposal.label").to_uppercase()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .truncate()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(title),
            )
            .child(
                div()
                    .debug_selector(|| "proposal-meta".into())
                    .truncate()
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(meta_line(record, cx)),
            )
            .child(brief_view)
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(hint),
            )
            .children(error.map(|error| {
                div()
                    .debug_selector(|| "proposal-error".into())
                    .mt(px(8.0))
                    .text_size(px(11.5))
                    .line_height(px(16.0))
                    .text_color(tw::RED_500.hsla())
                    .child(error)
            }))
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .child(start)
                    .children(edit)
                    .child(dismiss),
            ),
    )
    .into_any_element()
}
