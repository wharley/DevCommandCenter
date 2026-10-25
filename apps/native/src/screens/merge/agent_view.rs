//! The agent's part of the resolver, as the desktop draws it: the
//! "Resolver arquivo com agente" button, the run's sky panel (its state,
//! "Ver sessão", "Cancelar", what it did and its live output, a permission
//! or a question waiting, the error) and the violet note of the answer it
//! put in the editor.

use dcc_core::domain::merge_conflict::agent::{AgentActivity, agent_run_details};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::agent::AgentStatus;
use super::view::dark;
use super::{Busy, MergeResolver};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, spinner};

/// "Resolver arquivo com agente", among the file's actions.
pub(crate) fn resolve_button(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> AnyElement {
    let title = cx.t("mergeConflict.agent.resolveFileTitle");
    let working = resolver.busy == Some(Busy::Agent);
    Tooltip::new(
        "merge-resolve-with-agent-tip",
        Button::new("merge-resolve-with-agent")
            .variant(ButtonVariant::Secondary)
            .size(ButtonSize::Xs)
            .disabled(resolver.busy.is_some())
            .on_click(cx.listener(|resolver, _, _, cx| resolver.resolve_with_agent(cx)))
            .child(if working {
                spinner("merge-agent-busy", 14.0).into_any_element()
            } else {
                Icon::new("sparkles").size(14.0).into_any_element()
            })
            .child(cx.t("mergeConflict.agent.resolveFile")),
        move |_, _| {
            div()
                .max_w(px(280.0))
                .child(title.clone())
                .into_any_element()
        },
    )
    .into_any_element()
}

fn activity_text(activity: &AgentActivity, cx: &gpui::App) -> SharedString {
    match activity {
        AgentActivity::Started => cx.t("mergeConflict.agent.activity.started"),
        AgentActivity::Analyzing(Some(label)) => label.clone().into(),
        AgentActivity::Analyzing(None) => cx.t("mergeConflict.agent.activity.analyzing"),
        AgentActivity::Tool { action, target } => cx.t_with(
            "mergeConflict.agent.activity.tool",
            &Args::default().with("action", action.clone()).with(
                "target",
                target
                    .as_ref()
                    .map(|target| format!(" · {target}"))
                    .unwrap_or_default(),
            ),
        ),
        AgentActivity::ToolFailed(reason) => cx.t_with(
            "mergeConflict.agent.activity.toolFailed",
            &Args::default().with(
                "reason",
                reason
                    .as_ref()
                    .map(|reason| format!(" · {reason}"))
                    .unwrap_or_default(),
            ),
        ),
        AgentActivity::Permission => cx.t("mergeConflict.agent.activity.permission"),
        AgentActivity::Input => cx.t("mergeConflict.agent.activity.input"),
        AgentActivity::Completed => cx.t("mergeConflict.agent.activity.completed"),
        AgentActivity::Aborted => cx.t("mergeConflict.agent.activity.aborted"),
    }
}

fn run_panel(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> Option<AnyElement> {
    let run = resolver.agent.as_ref()?;
    let colors = cx.theme().colors.clone();
    let details = run
        .turn_id
        .as_deref()
        .map(|turn| agent_run_details(&run.events, turn))
        .unwrap_or_default();
    let (status_icon, label) = match run.status {
        AgentStatus::Analyzing => (
            div()
                .text_color(tw::SKY_500.hsla())
                .child(spinner("merge-agent-analyzing", 14.0))
                .into_any_element(),
            "mergeConflict.agent.run.analyzing",
        ),
        AgentStatus::Cancelling => (
            div()
                .text_color(tw::AMBER_500.hsla())
                .child(spinner("merge-agent-cancelling", 14.0))
                .into_any_element(),
            "mergeConflict.agent.run.cancelling",
        ),
        AgentStatus::Completed => (
            div()
                .text_color(tw::EMERALD_500.hsla())
                .child(Icon::new("check").size(14.0))
                .into_any_element(),
            "mergeConflict.agent.run.completed",
        ),
        AgentStatus::Failed => (
            div()
                .text_color(colors.destructive)
                .child(Icon::new("triangle-alert").size(14.0))
                .into_any_element(),
            "mergeConflict.agent.run.failed",
        ),
    };
    let running = matches!(run.status, AgentStatus::Analyzing | AgentStatus::Cancelling);
    let header = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .child(status_icon)
        .child(
            div()
                .mr_auto()
                .font_weight(FontWeight::SEMIBOLD)
                .child(cx.t(label)),
        )
        .children(run.session_id.is_some().then(|| {
            Button::new("merge-agent-session")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .text_size(11.0)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.request_agent_session(cx)))
                .child(Icon::new("external-link").size(14.0))
                .child(cx.t("mergeConflict.agent.viewSession"))
        }))
        .children(running.then(|| {
            Button::new("merge-agent-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .text_size(11.0)
                .disabled(run.status == AgentStatus::Cancelling)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.cancel_agent(cx)))
                .child(Icon::new("square").size(12.0))
                .child(cx.t("mergeConflict.agent.cancel"))
        }))
        .child(
            Button::new("merge-agent-toggle")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.toggle_agent_details(cx)))
                .child(
                    Icon::new(if run.expanded {
                        "chevron-up"
                    } else {
                        "chevron-down"
                    })
                    .size(14.0),
                ),
        );
    let waiting = (details.pending_permission || details.pending_input).then(|| {
        let key = if details.pending_permission {
            "mergeConflict.agent.permissionRequired"
        } else {
            "mergeConflict.agent.inputRequired"
        };
        div()
            .flex()
            .items_start()
            .gap(px(8.0))
            .border_t_1()
            .border_color(tw::AMBER_500.alpha(0.2))
            .bg(tw::AMBER_500.alpha(0.08))
            .px(px(12.0))
            .py(px(8.0))
            .text_color(
                if dark(cx) {
                    tw::AMBER_200
                } else {
                    tw::AMBER_800
                }
                .hsla(),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .child(Icon::new("triangle-alert").size(14.0)),
            )
            .child(cx.t(key))
    });
    let box_style = |content: AnyElement, id: &'static str, cx: &gpui::App| {
        let colors = &cx.theme().colors;
        div()
            .id(id)
            .max_h(px(112.0))
            .overflow_y_scroll()
            .rounded(px(4.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background.opacity(0.6))
            .px(px(8.0))
            .py(px(6.0))
            .text_color(colors.muted_foreground)
            .child(content)
    };
    let expanded = run.expanded.then(|| {
        let activities: AnyElement = if details.activities.is_empty() {
            div()
                .child(cx.t("mergeConflict.agent.activity.waiting"))
                .into_any_element()
        } else {
            div()
                .children(details.activities.iter().map(|activity| {
                    div()
                        .truncate()
                        .child(format!("• {}", activity_text(activity, cx)))
                }))
                .into_any_element()
        };
        let output: SharedString = if details.output.is_empty() {
            cx.t("mergeConflict.agent.waitingOutput")
        } else {
            details.output.clone().into()
        };
        div()
            .debug_selector(|| "merge-agent-details".into())
            .flex()
            .gap(px(8.0))
            .border_t_1()
            .border_color(colors.border.opacity(0.5))
            .p(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .mb(px(4.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(cx.t("mergeConflict.agent.activity.title")),
                    )
                    .child(box_style(activities, "merge-agent-activity", cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .mb(px(4.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground)
                            .child(cx.t("mergeConflict.agent.liveOutput")),
                    )
                    .child(box_style(
                        div()
                            .font_family(fonts::MONO)
                            .text_size(px(10.0))
                            .child(output)
                            .into_any_element(),
                        "merge-agent-output",
                        cx,
                    )),
            )
    });
    let error = run.error.clone().map(|error| {
        div()
            .debug_selector(|| "merge-agent-error".into())
            .border_t_1()
            .border_color(colors.destructive.opacity(0.2))
            .bg(colors.destructive.opacity(0.06))
            .px(px(12.0))
            .py(px(8.0))
            .text_color(colors.destructive)
            .child(error)
    });
    Some(
        div()
            .debug_selector(|| "merge-agent-run".into())
            .mt(px(8.0))
            .overflow_hidden()
            .rounded(px(6.0))
            .border_1()
            .border_color(tw::SKY_500.alpha(0.25))
            .bg(tw::SKY_500.alpha(0.04))
            .text_size(px(11.0))
            .child(header)
            .children(waiting)
            .children(expanded)
            .children(error)
            .into_any_element(),
    )
}

fn suggestion_note(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> Option<AnyElement> {
    let suggestion = resolver.suggestion.as_ref()?;
    let open = resolver.open.as_ref()?;
    if suggestion.path != open.entry.path {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let violet = if dark(cx) {
        tw::VIOLET_300
    } else {
        tw::VIOLET_700
    }
    .hsla();
    Some(
        div()
            .debug_selector(|| "merge-agent-suggestion".into())
            .mt(px(8.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(tw::VIOLET_500.alpha(0.25))
            .bg(tw::VIOLET_500.alpha(0.05))
            .px(px(12.0))
            .py(px(8.0))
            .text_size(px(11.0))
            .line_height(px(20.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(violet)
                    .child(Icon::new("sparkles").size(14.0))
                    .child(cx.t("mergeConflict.agent.suggestionAppliedFile")),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_color(colors.muted_foreground)
                    .child(suggestion.explanation.clone()),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(cx.t("mergeConflict.agent.reviewNotice")),
            )
            .into_any_element(),
    )
}

/// The run and the note, under the block panel's content.
pub(crate) fn agent_panels(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> Vec<AnyElement> {
    [run_panel(resolver, cx), suggestion_note(resolver, cx)]
        .into_iter()
        .flatten()
        .collect()
}
