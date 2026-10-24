//! The cards a turn carries besides its activity: the agent's questions
//! (`UserInputCard`), permission requests (`ApprovalCard`) and native
//! subagents (`NativeSubagentCard`). Read-only here: answering and approving
//! still happen in the desktop app.

use dcc_core::domain::provider::NativeSubagentStatus;
use dcc_core::domain::timeline::{Annotation, NativeSubagent};
use dcc_core::ports::provider::{ProviderUserInputAnswer, ProviderUserInputQuestion};
use gpui::{AnyElement, App, FontWeight, SharedString, div, prelude::*, px};

use crate::agent::{AgentMascot, RobotState};
use crate::i18n::Translate;
use crate::project::model_label;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Tracked};

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
        .children(badge)
        .into_any_element()
}

fn field_label(text: SharedString, cx: &App) -> AnyElement {
    eyebrow(text, 12.0, 0.18, 0.7, FontWeight::SEMIBOLD, cx)
}

pub(super) fn user_input_card(
    questions: &[ProviderUserInputQuestion],
    answers: &[ProviderUserInputAnswer],
    live: bool,
    cx: &App,
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
    let rows = questions.into_iter().map(|question| {
        let answer = answers
            .iter()
            .find(|answer| answer.question == question.question)
            .map(|answer| answer.answer.clone())
            .unwrap_or_default();
        let header = if question.header.trim().is_empty() {
            cx.t("conversation.userInput.question")
        } else {
            SharedString::from(question.header.clone())
        };
        let options = question.options.iter().map(|option| {
            let active = option.label == answer;
            div()
                .px(px(12.0))
                .py(px(8.0))
                .rounded(radius.control)
                .border_1()
                .border_color(if active {
                    colors.primary
                } else {
                    colors.border
                })
                .bg(if active {
                    colors.primary
                } else {
                    colors.background.opacity(0.6)
                })
                .text_color(if active {
                    colors.primary_foreground
                } else {
                    colors.foreground
                })
                .text_size(px(14.0))
                .opacity(if live { 1.0 } else { 0.5 })
                .child(SharedString::from(option.label.clone()))
        });
        let answer_box = div()
            .h(px(36.0))
            .flex()
            .items_center()
            .px(px(12.0))
            .rounded(radius.control)
            .border_1()
            .border_color(colors.input)
            .text_size(px(14.0))
            .opacity(0.5)
            .text_color(if answer.is_empty() {
                colors.muted_foreground
            } else {
                colors.foreground
            })
            .child(if answer.is_empty() {
                cx.t("conversation.userInput.placeholder")
            } else {
                SharedString::from(answer.clone())
            });
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
            .when(!question.options.is_empty(), |row| {
                row.child(div().flex().flex_wrap().gap(px(8.0)).children(options))
            })
            .child(answer_box)
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
            .into_any_element(),
    )
}

pub(super) struct ApprovalView<'a> {
    pub tool_name: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub command: Option<&'a str>,
    pub file: Option<&'a str>,
    pub behavior: Option<&'a str>,
    pub live: bool,
}

pub(super) fn approval_card(approval: ApprovalView<'_>, cx: &App) -> AnyElement {
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
    let title = SharedString::from(approval.title.unwrap_or(approval.tool_name).to_string());
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
