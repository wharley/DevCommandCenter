//! The second line of a task row: what the task is doing now
//! (`WorkspaceRailRowItem`'s state slot), from snooze to agent state.

use chrono::{Datelike, Local, TimeZone, Timelike};
use dcc_core::domain::attention::{AgentActivityState, AttentionReason};
use dcc_core::domain::usage_limit::{UsageLimitPhase, usage_limit_phase};
use gpui::{AnyElement, App, Hsla, SharedString, div, prelude::*, px};

use super::labels::{activity_time, amber_text, emerald_text, sky_text, violet_text};
use crate::data::activity::TaskActivity;
use crate::data::rail::is_snoozed;
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::data::time::parse_ms;
use crate::i18n::{Args, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::Icon;

pub struct RowContext<'a> {
    pub task: &'a TaskSummary,
    pub activity: Option<&'a TaskActivity>,
    pub unseen_result: bool,
    pub compact: bool,
    pub now_ms: i64,
}

/// The state slot, or `None` when nothing applies.
pub fn status(row: &RowContext, cx: &App) -> Option<AnyElement> {
    let task = row.task;
    let colors = &cx.theme().colors;
    // The git recap (next action, PR) arrives with its extraction; until
    // then a ready task with no agent reads "Ready to start".
    let has_recap = false;
    let priority_status = task.status == TaskStatus::Initializing
        || (task.status == TaskStatus::SetupPending && !has_recap);

    if is_snoozed(task, row.now_ms)
        && let Some(until) = task
            .snoozed_until
            .as_deref()
            .and_then(|at| snooze_label(at, cx))
    {
        let text = cx.t_with("sidebar.snooze.until", &Args::default().with("time", until));
        return Some(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .child(Icon::new("alarm-clock").size(12.0))
                .child(div().truncate().child(text))
                .into_any_element(),
        );
    }

    let status_message = match task.status {
        TaskStatus::Initializing => Some(cx.t("sidebar.workspaceState.initializing")),
        TaskStatus::SetupPending if priority_status => {
            Some(cx.t("sidebar.workspaceState.setupPending"))
        }
        TaskStatus::Ready if row.activity.is_none() && !has_recap => {
            Some(cx.t("sidebar.workspaceState.readyToStart"))
        }
        _ => None,
    };
    if let Some(message) = status_message.filter(|_| !row.compact) {
        let color = (task.status == TaskStatus::SetupPending).then(|| amber_text(cx));
        return Some(
            div()
                .truncate()
                .when_some(color, |text, color| text.text_color(color))
                .child(message)
                .into_any_element(),
        );
    }

    if row.compact || priority_status {
        return branch_label(task, colors.muted_foreground.opacity(0.7));
    }
    let Some(activity) = row.activity else {
        return branch_label(task, colors.muted_foreground.opacity(0.7));
    };

    if activity.activity.state == AgentActivityState::UsageLimited
        && let Some(limit) = &activity.activity.usage_limit
    {
        return Some(usage_limit(limit, row.now_ms, cx));
    }

    let (key, color) = match activity.activity.state {
        AgentActivityState::Active => ("sidebar.agentState.active", Some(emerald_text(cx))),
        AgentActivityState::Waiting(AttentionReason::Permission) => {
            ("sidebar.agentState.waitingPermission", Some(amber_text(cx)))
        }
        AgentActivityState::Waiting(_) => ("sidebar.agentState.waiting", Some(amber_text(cx))),
        AgentActivityState::Completed if row.unseen_result => {
            ("sidebar.agentState.completedUnseen", Some(sky_text(cx)))
        }
        AgentActivityState::Completed => (
            "sidebar.agentState.completed",
            Some(colors.foreground.opacity(0.7)),
        ),
        AgentActivityState::Aborted => (
            "sidebar.agentState.aborted",
            Some(colors.destructive.opacity(0.85)),
        ),
        AgentActivityState::UsageLimited => ("sidebar.agentState.limited", None),
    };
    let time = activity_time(activity, false, row.now_ms, cx);
    Some(
        div()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(4.0))
            .whitespace_nowrap()
            .child(
                div()
                    .truncate()
                    .when_some(color, |text, color| text.text_color(color))
                    .child(cx.t(key)),
            )
            .when_some(time, |line, time| {
                line.child(div().opacity(0.4).child("·"))
                    .child(div().font_features(crate::ui::tabular_nums()).child(time))
            })
            .into_any_element(),
    )
}

/// `workingBranchLabel` with the task's branch (git's live branch comes with
/// the recap).
fn branch_label(task: &TaskSummary, color: Hsla) -> Option<AnyElement> {
    let branch = task.branch.trim();
    let label = if branch.is_empty() {
        task.base_branch.trim()
    } else {
        branch
    };
    (!label.is_empty()).then(|| {
        div()
            .truncate()
            .text_color(color)
            .child(SharedString::from(label.to_string()))
            .into_any_element()
    })
}

/// `WorkspaceUsageLimitState`: violet while it waits for the reset, amber
/// once it waits for the person.
fn usage_limit(
    limit: &dcc_core::domain::usage_limit::SessionUsageLimit,
    now_ms: i64,
    cx: &App,
) -> AnyElement {
    let now = chrono::Utc
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(chrono::Utc::now);
    let paused = usage_limit_phase(limit, now).phase == UsageLimitPhase::Paused;
    let label = if paused {
        cx.t("sidebar.agentState.limitPaused")
    } else {
        cx.t("sidebar.agentState.limitWaiting")
    };
    let detail = (!paused).then(|| {
        let due = limit
            .resets_at
            .as_deref()
            .and_then(parse_ms)
            .is_some_and(|at| at <= now_ms);
        if due {
            Some(cx.t("usageLimit.resumingSoon"))
        } else {
            let time = limit
                .resets_at
                .as_deref()
                .and_then(|at| reset_label(at, now_ms))?;
            let key = if limit.auto_resume {
                "usageLimit.backAt"
            } else {
                "usageLimit.freesAt"
            };
            Some(cx.t_with(key, &Args::default().with("time", time)))
        }
    });
    let (dot, text) = if paused {
        (tw::AMBER_500.hsla(), amber_text(cx))
    } else {
        (tw::VIOLET_500.alpha(0.8), violet_text(cx))
    };
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .overflow_hidden()
        .child(div().size(px(6.0)).flex_shrink_0().rounded_full().bg(dot))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .whitespace_nowrap()
                .child(div().text_color(text).child(label))
                .when_some(detail.flatten(), |line, detail| {
                    line.child(div().opacity(0.4).child("·"))
                        .child(div().truncate().child(detail))
                }),
        )
        .into_any_element()
}

/// `formatLimitReset`: "03:40", with the date when it is not today.
fn reset_label(resets_at: &str, now_ms: i64) -> Option<String> {
    let at = Local.timestamp_millis_opt(parse_ms(resets_at)?).single()?;
    let today = Local.timestamp_millis_opt(now_ms).single()?;
    let time = format!("{:02}:{:02}", at.hour(), at.minute());
    Some(if at.date_naive() == today.date_naive() {
        time
    } else {
        format!("{:02}/{:02} {time}", at.day(), at.month())
    })
}

/// `Intl.DateTimeFormat(locale, { weekday: "short", hour, minute })`.
fn snooze_label(until: &str, cx: &App) -> Option<String> {
    let at = Local.timestamp_millis_opt(parse_ms(until)?).single()?;
    let weekday = at.weekday().num_days_from_sunday() as usize;
    Some(match cx.global::<crate::i18n::I18n>().locale() {
        Locale::PtBr => {
            const DAYS: [&str; 7] = ["dom.", "seg.", "ter.", "qua.", "qui.", "sex.", "sáb."];
            format!("{}, {:02}:{:02}", DAYS[weekday], at.hour(), at.minute())
        }
        Locale::En => {
            const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            let (pm, hour) = at.hour12();
            format!(
                "{} {hour:02}:{:02} {}",
                DAYS[weekday],
                at.minute(),
                if pm { "PM" } else { "AM" }
            )
        }
    })
}
