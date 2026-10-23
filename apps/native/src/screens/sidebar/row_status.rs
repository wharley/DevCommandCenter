//! The second line of a task row: what the task is doing now
//! (`WorkspaceRailRowItem`'s state slot), from snooze to agent state.

use chrono::{Datelike, Local, TimeZone, Timelike};
use dcc_core::domain::attention::{AgentActivityState, AttentionReason};
use dcc_core::domain::usage_limit::{UsageLimitPhase, usage_limit_phase};
use gpui::{AnyElement, App, Hsla, SharedString, div, prelude::*, px};

use super::labels::{activity_time, amber_text, emerald_text, sky_text, violet_text};
use super::state::RowGit;
use crate::data::activity::TaskActivity;
use crate::data::rail::is_snoozed;
use crate::data::recap::{Recap, RecapTone, next_action_key};
use crate::data::tasks::{TaskStatus, TaskSummary};
use crate::data::time::parse_ms;
use crate::i18n::{Args, Locale, Translate};
use crate::theme::ActiveTheme;
use crate::theme::tailwind as tw;
use crate::ui::Icon;

pub struct RowContext<'a> {
    pub task: &'a TaskSummary,
    pub activity: Option<&'a TaskActivity>,
    pub git: &'a RowGit,
    /// Delegated edits waiting on the person.
    pub pending_reviews: usize,
    pub unseen_result: bool,
    pub compact: bool,
    pub now_ms: i64,
}

fn recap_args(recap: &Recap) -> Args {
    Args::count(recap.count)
        .with("additions", recap.additions.to_string())
        .with("deletions", recap.deletions.to_string())
        .with("pr", recap.pr.clone())
        .with("requestLabel", recap.request_label)
}

/// `recapToneClass`: graded by value; only attention spends chroma.
fn tone_color(tone: RecapTone, cx: &App) -> Hsla {
    let colors = &cx.theme().colors;
    match tone {
        RecapTone::Neutral | RecapTone::Done => colors.muted_foreground.opacity(0.8),
        RecapTone::Working => colors.muted_foreground,
        RecapTone::Attention => amber_text(cx),
        RecapTone::Ready => colors.foreground.opacity(0.7),
    }
}

/// The state slot, in the desktop's order: snooze, workspace status, usage
/// limit, the next action between turns, the agent's state, the branch.
pub fn status(row: &RowContext, cx: &App) -> Option<AnyElement> {
    let task = row.task;
    let colors = &cx.theme().colors;
    let recap = row.git.recap.as_ref();
    let has_recap = recap.is_some();
    let priority_status = task.status == TaskStatus::Initializing
        || (task.status == TaskStatus::SetupPending && !has_recap);
    let state = row.activity.map(|activity| activity.activity.state);
    // The agent finished, but edits it delegated wait on the person.
    let awaits_review = state == Some(AgentActivityState::Completed) && row.pending_reviews > 0;

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

    if !row.compact
        && !priority_status
        && let Some(activity) = row.activity
        && activity.activity.state == AgentActivityState::UsageLimited
        && let Some(limit) = &activity.activity.usage_limit
    {
        return Some(usage_limit(limit, row.now_ms, cx));
    }

    // Between turns the next step says more than "Completed".
    let next_action = recap.and_then(|recap| {
        let key = next_action_key(recap.message_key)?;
        let args = recap_args(recap);
        let message = cx.t_with(
            &format!("inspector.recap.messages.{}", recap.message_key),
            &args,
        );
        let i18n = cx.global::<crate::i18n::I18n>();
        Some(if i18n.has(&key, &args) {
            cx.t_with(&key, &args)
        } else {
            message
        })
    });
    let shows_next_action = !row.compact
        && !priority_status
        && !awaits_review
        && !matches!(
            state,
            Some(
                AgentActivityState::Active
                    | AgentActivityState::Waiting(_)
                    | AgentActivityState::UsageLimited
            )
        );
    if shows_next_action && let (Some(text), Some(recap)) = (next_action, recap) {
        return Some(
            div()
                .truncate()
                .text_color(tone_color(recap.tone, cx))
                .child(text)
                .into_any_element(),
        );
    }

    if let Some(activity) = row.activity.filter(|_| !row.compact && !priority_status) {
        let (key, color) = match activity.activity.state {
            AgentActivityState::Active => ("sidebar.agentState.active", Some(emerald_text(cx))),
            AgentActivityState::Waiting(AttentionReason::Permission) => {
                ("sidebar.agentState.waitingPermission", Some(amber_text(cx)))
            }
            AgentActivityState::Waiting(_) => ("sidebar.agentState.waiting", Some(amber_text(cx))),
            AgentActivityState::Completed if awaits_review => {
                ("sidebar.agentState.reviewPending", Some(amber_text(cx)))
            }
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
        return Some(
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
        );
    }

    branch_label(
        &row.git.current_branch,
        &task.branch,
        colors.muted_foreground.opacity(0.7),
        cx,
    )
}

/// `workingBranchLabel`: git answers "HEAD" for a detached checkout (how a
/// task worktree starts); that reads "no branch" next to its base.
fn branch_label(current: &str, base: &str, color: Hsla, cx: &App) -> Option<AnyElement> {
    let base = Some(base.trim()).filter(|base| !base.is_empty());
    let label: SharedString = if current == "HEAD" {
        match base.filter(|base| *base != "HEAD") {
            Some(base) => cx.t_with(
                "workingBranch.detached",
                &Args::default().with("branch", base),
            ),
            None => cx.t("workingBranch.detachedNoBase"),
        }
    } else {
        let current = current.trim();
        if current.is_empty() {
            SharedString::from(base?.to_string())
        } else {
            SharedString::from(current.to_string())
        }
    };
    Some(
        div()
            .truncate()
            .text_color(color)
            .child(label)
            .into_any_element(),
    )
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
