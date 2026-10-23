//! Words and colors the rail derives from state: elapsed times
//! (`WorkspaceActivityTime`), "Needs you" reasons and their dot/text colors.

use dcc_core::domain::attention::AttentionReason;
use gpui::{App, Hsla, SharedString};

use crate::data::activity::TaskActivity;
use crate::data::time::{ElapsedUnits, compact_elapsed, parse_ms};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};

fn units(cx: &App) -> ElapsedUnits {
    let unit = |name: &str| {
        cx.t(&format!("sidebar.activityTime.units.{name}"))
            .to_string()
    };
    ElapsedUnits {
        second: unit("second"),
        minute: unit("minute"),
        hour: unit("hour"),
        day: unit("day"),
        month: unit("month"),
        months: unit("months"),
    }
}

/// "5min" since `at` (no "há"), for the identity card's fallback.
pub fn elapsed_since(at: &str, now_ms: i64, cx: &App) -> Option<SharedString> {
    Some(compact_elapsed(now_ms - parse_ms(at)?, &units(cx)).into())
}

/// "5min" while the turn is open or `bare`, else "há 5min".
pub fn activity_time(
    activity: &TaskActivity,
    bare: bool,
    now_ms: i64,
    cx: &App,
) -> Option<SharedString> {
    let at = parse_ms(activity.timestamp()?)?;
    let time = compact_elapsed(now_ms - at, &units(cx));
    Some(if activity.is_turn_open() || bare {
        time.into()
    } else {
        cx.t_with(
            "sidebar.activityTime.ago",
            &Args::default().with("time", time),
        )
    })
}

pub fn reason_key(reason: AttentionReason) -> &'static str {
    match reason {
        AttentionReason::Permission => "permission",
        AttentionReason::Input => "input",
        AttentionReason::UsageLimit => "usageLimit",
        AttentionReason::Conflicts => "conflicts",
        AttentionReason::PrConflicts => "prConflicts",
        AttentionReason::ChecksFailing => "checksFailing",
        AttentionReason::DelegatedReview => "delegatedReview",
        AttentionReason::Woke => "woke",
        AttentionReason::Setup => "setup",
        AttentionReason::Aborted => "aborted",
        AttentionReason::Completed => "completed",
    }
}

pub fn reason_label(reason: AttentionReason, count: Option<u64>, cx: &App) -> SharedString {
    let count = count.unwrap_or(0) as i64;
    let args = Args::count(count).with("pr", count.to_string());
    cx.t_with(
        &format!("sidebar.attention.reason.{}", reason_key(reason)),
        &args,
    )
}

/// Amber blocks on you, red was interrupted, sky is a result not opened yet.
pub fn reason_dot(reason: AttentionReason, cx: &App) -> Hsla {
    let destructive = cx.theme().colors.destructive;
    match reason {
        AttentionReason::ChecksFailing | AttentionReason::Aborted => destructive,
        AttentionReason::Woke | AttentionReason::Completed => tw::SKY_500.hsla(),
        AttentionReason::Setup => tw::AMBER_500.alpha(0.8),
        _ => tw::AMBER_500.hsla(),
    }
}

pub fn reason_text(reason: AttentionReason, cx: &App) -> Hsla {
    match reason {
        AttentionReason::ChecksFailing | AttentionReason::Aborted => {
            cx.theme().colors.destructive.opacity(0.85)
        }
        AttentionReason::Woke | AttentionReason::Completed => sky_text(cx),
        _ => amber_text(cx),
    }
}

/// `text-amber-700 dark:text-amber-300/90`
pub fn amber_text(cx: &App) -> Hsla {
    match cx.theme().appearance {
        Appearance::Light => tw::AMBER_700.hsla(),
        Appearance::Dark => tw::AMBER_300.alpha(0.9),
    }
}

/// `text-sky-700 dark:text-sky-300/90`
pub fn sky_text(cx: &App) -> Hsla {
    match cx.theme().appearance {
        Appearance::Light => tw::SKY_700.hsla(),
        Appearance::Dark => tw::SKY_300.alpha(0.9),
    }
}

/// `text-emerald-700 dark:text-emerald-300/90`
pub fn emerald_text(cx: &App) -> Hsla {
    match cx.theme().appearance {
        Appearance::Light => tw::EMERALD_700.hsla(),
        Appearance::Dark => tw::EMERALD_300.alpha(0.9),
    }
}

/// `text-violet-700 dark:text-violet-300/90`
pub fn violet_text(cx: &App) -> Hsla {
    match cx.theme().appearance {
        Appearance::Light => tw::VIOLET_700.hsla(),
        Appearance::Dark => tw::VIOLET_300.alpha(0.9),
    }
}
