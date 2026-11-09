//! The cards a turn carries besides its activity: the agent's questions
//! (`UserInputCard`), permission requests (`ApprovalCard`) and native
//! subagents (`NativeSubagentCard`). Answers go to the desktop, which holds
//! the agent (`requests`).

use dcc_core::domain::provider::NativeSubagentStatus;
use dcc_core::domain::timeline::{Annotation, NativeSubagent};
use dcc_core::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};
use gpui::{AnyElement, App, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::ConversationView;
use super::requests::Sending;

use crate::agent::{AgentMascot, RobotState};
use crate::i18n::Translate;
use crate::project::model_label;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonVariant, Icon, Tracked, spinner};

/// `rounded-2xl border-border/70 bg-card/70 p-4`.
fn card(cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .rounded(px(16.0))
        .border_1()
        .border_color(colors.border.opacity(0.7))
        .bg(colors.card.opacity(0.7))
        .p(px(16.0))
}

/// `text-[11px] font-medium uppercase tracking-[0.22em] text-muted-foreground/80`.
fn eyebrow(
    text: SharedString,
    size: f32,
    tracking: f32,
    opacity: f32,
    weight: FontWeight,
    cx: &App,
) -> AnyElement {
    div()
        .font_weight(weight)
        .text_color(cx.theme().colors.muted_foreground.opacity(opacity))
        .child(Tracked::new(text.to_uppercase(), size, tracking))
        .into_any_element()
}

fn card_header(
    label: SharedString,
    line: SharedString,
    badge: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .mb(px(12.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(eyebrow(label, 11.0, 0.22, 0.8, FontWeight::MEDIUM, cx))
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(colors.foreground)
                        .child(line),
                ),
        )
        .children(badge.map(|badge| div().flex_none().child(badge)))
        .into_any_element()
}

fn field_label(text: SharedString, cx: &App) -> AnyElement {
    eyebrow(text, 12.0, 0.18, 0.7, FontWeight::SEMIBOLD, cx)
}

pub(super) fn user_input_card(
    view: &ConversationView,
    request_id: &str,
    questions: &[ProviderUserInputQuestion],
    answers: &[ProviderUserInputAnswer],
    live: bool,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> Option<AnyElement> {
    let questions: Vec<&ProviderUserInputQuestion> = questions
        .iter()
        .filter(|question| !question.question.trim().is_empty())
        .collect();
    if questions.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let resolved = !live && !answers.is_empty();
    let sending = view.sending(request_id).is_some();
    let editable = live && !sending;
    let badge = resolved.then(|| {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(12.0))
            .text_color(crate::theme::tailwind::EMERALD_600.hsla())
            .child(Icon::new("circle-check").size(16.0))
            .child(cx.t("conversation.userInput.resolved"))
            .into_any_element()
    });
    let line = if resolved {
        cx.t("conversation.userInput.delivered")
    } else {
        cx.t("conversation.userInput.waiting")
    };
    let mut rows: Vec<AnyElement> = Vec::with_capacity(questions.len());
    let mut complete = true;
    for (index, question) in questions.iter().enumerate() {
        let stored = answers
            .iter()
            .find(|answer| answer.question == question.question)
            .map(|answer| answer.answer.clone())
            .unwrap_or_default();
        let input =
            live.then(|| view.answer_input(request_id, &question.question, &stored, window, cx));
        let selected = if live {
            view.answer_text(request_id, &question.question, cx)
        } else {
            stored.clone()
        };
        complete &= !selected.is_empty();
        let header = if question.header.trim().is_empty() {
            cx.t("conversation.userInput.question")
        } else {
            SharedString::from(question.header.clone())
        };
        let options: Vec<AnyElement> = question
            .options
            .iter()
            .enumerate()
            .map(|(option_index, option)| {
                let active = option.label == selected;
                let label = option.label.clone();
                let request = request_id.to_string();
                let asked = question.question.clone();
                Button::new(SharedString::from(format!(
                    "{request_id}-q{index}-o{option_index}"
                )))
                .variant(if active {
                    ButtonVariant::Default
                } else {
                    ButtonVariant::Outline
                })
                .disabled(!editable)
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.pick_option(&request, &asked, label.clone(), cx)
                }))
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(SharedString::from(option.label.clone())),
                )
                .into_any_element()
            })
            .collect();
        let field = div()
            .h(px(36.0))
            .flex()
            .items_center()
            .px(px(12.0))
            .rounded(radius.control)
            .border_1()
            .border_color(colors.input)
            .text_size(px(14.0))
            .when(!editable, |field| field.opacity(0.5));
        let field = match input {
            Some(input) if editable => field.child(div().flex_1().min_w_0().child(input)),
            _ => field
                .text_color(if selected.is_empty() {
                    colors.muted_foreground
                } else {
                    colors.foreground
                })
                .child(if selected.is_empty() {
                    cx.t("conversation.userInput.placeholder")
                } else {
                    SharedString::from(selected.clone())
                }),
        };
        rows.push(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(field_label(header, cx))
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(14.0))
                                .text_color(colors.foreground)
                                .child(SharedString::from(question.question.clone())),
                        ),
                )
                .when(!options.is_empty(), |row| {
                    row.child(div().flex().flex_wrap().gap(px(8.0)).children(options))
                })
                .child(field)
                .into_any_element(),
        );
    }
    let submit = live.then(|| {
        let request = request_id.to_string();
        let asked: Vec<String> = questions
            .iter()
            .map(|question| question.question.clone())
            .collect();
        div()
            .mt(px(16.0))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new(SharedString::from(format!("{request_id}-send")))
                    .disabled(!complete || sending)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.send_answers(request.clone(), asked.clone(), cx)
                    }))
                    .children(sending.then(|| {
                        spinner(SharedString::from(format!("{request_id}-sending")), 16.0)
                    }))
                    .child(if sending {
                        cx.t("conversation.userInput.sending")
                    } else {
                        cx.t("conversation.userInput.send")
                    }),
            )
    });
    Some(
        card(cx)
            .child(card_header(
                cx.t("conversation.userInput.label"),
                line,
                badge,
                cx,
            ))
            .child(div().flex().flex_col().gap(px(16.0)).children(rows))
            .children(submit)
            .into_any_element(),
    )
}

pub(super) struct ApprovalView<'a> {
    pub request_id: &'a str,
    pub tool_name: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub command: Option<&'a str>,
    pub file: Option<&'a str>,
    pub behavior: Option<&'a str>,
    pub live: bool,
}

pub(super) fn approval_card(
    view: &ConversationView,
    approval: ApprovalView<'_>,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let mono = theme.fonts.mono.clone();
    let radius = theme.radius;
    let resolved = !approval.live
        && approval
            .behavior
            .is_some_and(|behavior| !behavior.is_empty());
    let badge = resolved.then(|| {
        let allowed = approval.behavior == Some("allow");
        let label = match approval.behavior {
            Some("allow") => cx.t("conversation.permission.allowed"),
            Some("deny") => cx.t("conversation.permission.denied"),
            _ => cx.t("conversation.permission.pending"),
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(12.0))
            .text_color(colors.muted_foreground)
            .child(if allowed {
                Icon::new("circle-check")
                    .size(16.0)
                    .color(crate::theme::tailwind::EMERALD_600.hsla())
            } else {
                Icon::new("ban")
                    .size(16.0)
                    .color(crate::theme::tailwind::AMBER_600.hsla())
            })
            .child(label)
            .into_any_element()
    });
    let title = SharedString::from(
        approval
            .title
            .unwrap_or_else(|| super::approval_reason::short_tool(approval.tool_name))
            .to_string(),
    );
    card(cx)
        .child(card_header(
            cx.t("conversation.permission.label"),
            title,
            badge,
            cx,
        ))
        .children(approval.description.map(|description| {
            div()
                .mb(px(12.0))
                .text_size(px(14.0))
                .text_color(colors.muted_foreground)
                .child(SharedString::from(description.to_string()))
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(field_label(cx.t("conversation.permission.tool"), cx))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(colors.foreground)
                        .child(SharedString::from(approval.tool_name.to_string())),
                )
                .children(approval.command.map(|command| {
                    div()
                        .flex()
                        .flex_col()
                        .child(field_label(cx.t("conversation.permission.command"), cx))
                        .child(
                            div()
                                .mt(px(4.0))
                                .rounded(radius.xl)
                                .bg(colors.background.opacity(0.7))
                                .px(px(12.0))
                                .py(px(8.0))
                                .font_family(mono.clone())
                                .text_size(px(12.0))
                                .text_color(colors.foreground)
                                .child(SharedString::from(command.to_string())),
                        )
                }))
                .children(approval.file.map(|file| {
                    div()
                        .flex()
                        .flex_col()
                        .child(field_label(cx.t("conversation.permission.file"), cx))
                        .child(
                            div()
                                .mt(px(4.0))
                                .font_family(mono.clone())
                                .text_size(px(12.0))
                                .text_color(colors.foreground)
                                .child(SharedString::from(file.to_string())),
                        )
                })),
        )
        .children(approval.live.then(|| {
            let sending = view.sending(approval.request_id);
            let request = approval.request_id.to_string();
            let deny_request = request.clone();
            let tool = approval.tool_name.to_string();
            let busy = |kind: Sending| {
                (sending == Some(kind))
                    .then(|| spinner(SharedString::from(format!("{request}-{kind:?}")), 16.0))
            };
            // "Recusar com motivo": the field and its send, while open.
            let reason = view.reason_input(&request).map(|input| {
                let (send_request, send_tool) = (request.clone(), tool.clone());
                div()
                    .mt(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "permission-reason".into())
                            .flex_1()
                            .min_w_0()
                            .h(px(32.0))
                            .px(px(10.0))
                            .flex()
                            .items_center()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.input)
                            .bg(crate::ui::input_fill(cx.theme()))
                            .text_size(px(13.0))
                            .child(input),
                    )
                    .child(
                        Button::new(SharedString::from(format!("{request}-deny-send")))
                            .variant(ButtonVariant::Outline)
                            .disabled(sending.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.deny_with_reason(send_request.clone(), send_tool.clone(), cx)
                            }))
                            .child(cx.t("conversation.permission.denySend")),
                    )
            });
            let reason_open = reason.is_some();
            let (reason_request, always_request, always_tool) =
                (request.clone(), request.clone(), tool.clone());
            let always = super::approval_reason::may_always_allow(&tool).then(|| {
                Button::new(SharedString::from(format!("{request}-always")))
                    .variant(ButtonVariant::Outline)
                    .disabled(sending.is_some())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.allow_always(always_request.clone(), always_tool.clone(), cx)
                    }))
                    .child(cx.t("conversation.permission.always"))
            });
            div().children(reason).child(
                div()
                    .mt(px(16.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .when(!reason_open, |row| {
                        row.child(
                            Button::new(SharedString::from(format!(
                                "{reason_request}-deny-reason"
                            )))
                            .variant(ButtonVariant::Ghost)
                            .disabled(sending.is_some())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.open_reason(reason_request.clone(), window, cx)
                            }))
                            .child(cx.t("conversation.permission.denyWithReason")),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("{request}-deny")))
                            .variant(ButtonVariant::Outline)
                            .disabled(sending.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.respond_permission(deny_request.clone(), false, cx)
                            }))
                            .children(busy(Sending::Deny))
                            .child(cx.t("conversation.permission.deny")),
                    )
                    .child({
                        let allow_request = request.clone();
                        Button::new(SharedString::from(format!("{request}-allow")))
                            .disabled(sending.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.respond_permission(allow_request.clone(), true, cx)
                            }))
                            .children(busy(Sending::Allow))
                            .child(cx.t("conversation.permission.allow"))
                    })
                    .children(always),
            )
        }))
        .into_any_element()
}

/// `nativeSubagentDisplayStatus`: once the parent settles, a child still
/// marked running reads as settled.
fn display_status(agent: &NativeSubagent, parent_streaming: bool) -> &'static str {
    match agent.status {
        NativeSubagentStatus::Running if !parent_streaming => "settled",
        NativeSubagentStatus::Running => "running",
        NativeSubagentStatus::Completed => "completed",
        NativeSubagentStatus::Failed => "failed",
    }
}

pub(super) fn native_subagent_card(
    agent: &NativeSubagent,
    provider: Option<&str>,
    parent_streaming: bool,
    frame: bool,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let status = display_status(agent, parent_streaming);
    let identity = agent
        .model
        .as_deref()
        .map(model_label)
        .or_else(|| agent.requested_model.as_deref().map(model_label))
        .or_else(|| agent.name.clone())
        .or_else(|| agent.role.clone());
    let details: Vec<String> = [agent.name.clone(), agent.role.clone()]
        .into_iter()
        .flatten()
        .filter(|value| Some(value) != identity.as_ref())
        .fold(Vec::new(), |mut all, value| {
            if !all.contains(&value) {
                all.push(value);
            }
            all
        });
    let state = if status == "running" {
        RobotState::Working
    } else {
        RobotState::Idle
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w_0()
        .mb(px(8.0))
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .bg(colors.muted.opacity(0.15))
        .px(px(10.0))
        .py(px(8.0))
        .text_size(px(12.0))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .min_w_0()
                .child(AgentMascot::new(provider, agent.model.as_deref(), state).moving(frame))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .min_w_0()
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground.opacity(0.85))
                                        .child(cx.t("conversation.nativeSubagent.label")),
                                )
                                .children(identity.map(|identity| {
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(colors.foreground.opacity(0.85))
                                        .child(identity)
                                })),
                        )
                        .when(!details.is_empty(), |column| {
                            column.child(
                                div()
                                    .truncate()
                                    .text_color(colors.muted_foreground)
                                    .child(details.join(" · ")),
                            )
                        }),
                )
                .child(
                    div()
                        .ml_auto()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t(&format!("conversation.nativeSubagent.status.{status}"))),
                ),
        )
        .into_any_element()
}

/// Whether a request still waits on the person.
pub(super) fn waits_on_person(annotation: &Annotation) -> bool {
    matches!(
        annotation,
        Annotation::UserInput {
            streaming: true,
            ..
        } | Annotation::Approval {
            streaming: true,
            ..
        }
    )
}
