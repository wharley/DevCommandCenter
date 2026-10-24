//! `AssistantMessage`: one turn of the agent, left aligned under
//! `max-w-[52rem]` — the model and its robot, the turn's activity, the
//! subagents and requests it raised, the answer, the footer and, when the
//! turn stopped short, why.

use dcc_core::domain::timeline::activity::{
    TurnTokenTotals, format_elapsed, format_token_count, is_activity, turn_duration_ms,
};
use dcc_core::domain::timeline::{Annotation, TimelineMessage};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::activity::{TurnActivity, turn_activity};
use super::cards::{
    ApprovalView, approval_card, native_subagent_card, user_input_card, waits_on_person,
};
use super::hover_actions::{MESSAGE_GROUP, copy_action};
use super::{ConversationView, time_ago};
use crate::agent::{AgentMascot, RobotState};
use crate::data::conversation::Conversation;
use crate::i18n::{Args, I18n, Locale, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::project::model_label;
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::{Icon, Tooltip};

/// `toLocaleString()` for a token count: `12.345` in Portuguese, `12,345` in English.
fn grouped(value: u64, english: bool) -> String {
    let digits = value.to_string();
    let separator = if english { ',' } else { '.' };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(digit);
    }
    out
}

fn token_breakdown(totals: &TurnTokenTotals, cx: &gpui::App) -> String {
    let english = cx.global::<I18n>().locale() == Locale::En;
    let line = |key: &str, value: u64| {
        cx.t_with(key, &Args::default().with("value", grouped(value, english)))
            .to_string()
    };
    let mut lines = vec![
        line("conversation.turnFooter.tokensIn", totals.input),
        line("conversation.turnFooter.tokensOut", totals.output),
    ];
    if totals.cached > 0 {
        lines.push(line("conversation.turnFooter.tokensCached", totals.cached));
    }
    if totals.reasoning > 0 {
        lines.push(line(
            "conversation.turnFooter.tokensReasoning",
            totals.reasoning,
        ));
    }
    if let Some(cost) = totals.cost_usd {
        lines.push(
            cx.t_with(
                "conversation.turnFooter.cost",
                &Args::default().with("value", format!("{cost:.4}")),
            )
            .to_string(),
        );
    }
    lines.join("\n")
}

/// `TurnFooter`: when, how long and, once settled, the tokens it used.
fn footer(
    view: &ConversationView,
    data: &Conversation,
    message: &TimelineMessage,
    cx: &gpui::App,
) -> AnyElement {
    let colors = &cx.theme().colors;
    let stamp = time_ago(view, message.created_at.as_deref(), cx);
    let duration = (!message.streaming)
        .then(|| {
            turn_duration_ms(
                message.turn_started_at.as_deref(),
                message.turn_ended_at.as_deref(),
            )
        })
        .flatten()
        .map(format_elapsed);
    let tokens = (!message.streaming)
        .then(|| {
            message
                .turn_id
                .as_ref()
                .and_then(|turn| data.turn_tokens.get(turn))
        })
        .flatten();
    let separated = |content: AnyElement| div().flex().gap(px(6.0)).child("·").child(content);
    let id = &message.id;
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .mt(px(4.0))
        .text_size(px(11.0))
        .line_height(px(11.0))
        .text_color(colors.muted_foreground.opacity(0.7))
        .font_features(crate::ui::tabular_nums())
        .children(stamp)
        .children(duration.map(|duration| {
            separated(
                Tooltip::text(
                    SharedString::from(format!("{id}-duration")),
                    div().child(duration),
                    cx.t("conversation.turnFooter.durationHint"),
                )
                .into_any_element(),
            )
        }))
        .children(tokens.map(|totals| {
            let label = cx.t_with(
                "conversation.turnFooter.tokens",
                &Args::default()
                    .with("input", format_token_count(totals.input))
                    .with("output", format_token_count(totals.output)),
            );
            separated(
                Tooltip::text(
                    SharedString::from(format!("{id}-tokens")),
                    div().child(label),
                    token_breakdown(totals, cx),
                )
                .into_any_element(),
            )
        }))
        .into_any_element()
}

fn incomplete_notice(message: &TimelineMessage, cx: &gpui::App) -> Option<AnyElement> {
    let status = message.status.as_ref()?;
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let dark = theme.appearance == crate::theme::Appearance::Dark;
    let radius = theme.radius;
    let notice = div()
        .mt(px(8.0))
        .flex()
        .items_start()
        .gap(px(10.0))
        .max_w(px(672.0))
        .rounded(radius.lg)
        .border_1()
        .px(px(12.0))
        .py(px(10.0));
    if status.usage_limited {
        // A wait, not an error: the strip above the composer says when it
        // comes back.
        let continued = status.continued_on_account.as_ref().map(|account| {
            let name = if account.is_empty() {
                cx.t("usageLimit.defaultAccount").to_string()
            } else {
                account.clone()
            };
            div()
                .text_color(colors.muted_foreground)
                .font_weight(FontWeight::NORMAL)
                .child(format!(
                    " · {}",
                    cx.t_with(
                        "usageLimit.timelineContinuedOn",
                        &Args::default().with("name", name)
                    )
                ))
        });
        return Some(
            notice
                .border_color(tw::VIOLET_500.alpha(0.25))
                .bg(tw::VIOLET_500.alpha(0.05))
                .child(
                    div()
                        .mt(px(2.0))
                        .child(Icon::new("hourglass").size(16.0).color(if dark {
                            tw::VIOLET_300.hsla()
                        } else {
                            tw::VIOLET_600.hsla()
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.foreground)
                                .child(cx.t("usageLimit.timelinePaused"))
                                .children(continued),
                        )
                        .children(status.reason.clone().map(|reason| {
                            div()
                                .mt(px(2.0))
                                .text_size(px(12.0))
                                .text_color(colors.muted_foreground)
                                .child(reason)
                        })),
                )
                .into_any_element(),
        );
    }
    let reason = status
        .reason
        .clone()
        .filter(|reason| !reason.is_empty())
        .map(SharedString::from)
        .unwrap_or_else(|| cx.t("conversation.message.incomplete"));
    // Retry and continue arrive with the composer (screen 2c).
    Some(
        notice
            .border_color(colors.destructive.opacity(0.2))
            .bg(colors.destructive.opacity(0.045))
            .child(
                div().mt(px(2.0)).child(
                    Icon::new("circle-alert")
                        .size(16.0)
                        .color(colors.destructive),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(cx.t("conversation.message.interrupted")),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .text_size(px(12.0))
                            .line_height(px(20.0))
                            .text_color(colors.muted_foreground)
                            .child(reason),
                    ),
            )
            .into_any_element(),
    )
}

pub(super) fn assistant_row(
    view: &ConversationView,
    data: &Conversation,
    message: &TimelineMessage,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let provider = data.provider_id.as_deref();
    let model = message.model.as_deref().or(data.model.as_deref());
    let streaming = message.streaming;
    let interrupted = message.status.is_some();
    let waiting = message.annotations.iter().any(waits_on_person);
    let mascot_state = if waiting {
        RobotState::Waiting
    } else if streaming {
        RobotState::Working
    } else {
        RobotState::Idle
    };
    let has_activity = message.annotations.iter().any(is_activity);
    // While the live status line carries the walking robot, the header one
    // stands still so only one moves.
    let status_carries_mascot = streaming && !interrupted && has_activity;
    let frame = view.mascot_frame();
    let colors = cx.theme().colors.clone();

    let mut column: Vec<AnyElement> = Vec::new();
    if let Some(model) = model.filter(|model| !model.trim().is_empty()) {
        let header_state = if status_carries_mascot {
            RobotState::Idle
        } else {
            mascot_state
        };
        column.push(
            div()
                .mb(px(8.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(AgentMascot::new(provider, Some(model), header_state).moving(frame))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground.opacity(0.8))
                        .child(SharedString::from(model_label(model))),
                )
                .into_any_element(),
        );
    }
    if has_activity {
        let live_mascot = status_carries_mascot.then(|| {
            AgentMascot::new(provider, model, mascot_state)
                .moving(frame)
                .into_any_element()
        });
        column.push(turn_activity(
            view,
            TurnActivity {
                key: &message.id,
                annotations: &message.annotations,
                streaming,
                interrupted,
                waiting,
                started_at: message.turn_started_at.as_deref(),
                ended_at: message.turn_ended_at.as_deref(),
                live_mascot,
            },
            window,
            cx,
        ));
    }
    let subagents: Vec<AnyElement> = message
        .annotations
        .iter()
        .filter_map(|annotation| match annotation {
            Annotation::NativeSubagent(agent) => {
                Some(native_subagent_card(agent, provider, streaming, frame, cx))
            }
            _ => None,
        })
        .collect();
    column.extend(subagents);
    // A permission still waiting shows in the panel above the composer
    // (`hidePendingApprovals`); answered ones stay in the thread.
    let mut requests: Vec<AnyElement> = Vec::new();
    for annotation in &message.annotations {
        match annotation {
            Annotation::UserInput {
                id,
                questions,
                answers,
                streaming,
                ..
            } => requests.extend(user_input_card(
                view, id, questions, answers, *streaming, window, cx,
            )),
            Annotation::Approval {
                id,
                tool_name,
                title,
                description,
                command,
                file,
                behavior,
                streaming: false,
                ..
            } => requests.push(approval_card(
                view,
                ApprovalView {
                    request_id: id,
                    tool_name,
                    title: title.as_deref(),
                    description: description.as_deref(),
                    command: command.as_deref(),
                    file: file.as_deref(),
                    behavior: behavior.as_deref(),
                    live: false,
                },
                cx,
            )),
            _ => {}
        }
    }
    if !requests.is_empty() {
        column.push(
            div()
                .mb(px(8.0))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children(requests)
                .into_any_element(),
        );
    }
    // Plan cards and mission reports keep their own layouts (later slices);
    // until then their text reads as the answer.
    if !message.content.trim().is_empty() {
        let blocks = view.markdown(&message.content);
        column.push(
            div()
                .min_w_0()
                .text_color(colors.foreground)
                .child(render_markdown(
                    &blocks,
                    MarkdownStyle::reply(),
                    &message.id,
                    window,
                    cx,
                ))
                .into_any_element(),
        );
    }
    column.push(footer(view, data, message, cx));
    if let Some(notice) = incomplete_notice(message, cx) {
        column.push(notice);
    }
    div()
        .group(MESSAGE_GROUP)
        .flex()
        .min_w_0()
        .justify_start()
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .w_full()
                .max_w(px(832.0))
                .min_w_0()
                .pb(px(20.0))
                .children(column)
                .child(copy_action(
                    SharedString::from(format!("{}-copy", message.id)),
                    message.content.clone(),
                    cx,
                )),
        )
        .into_any_element()
}
