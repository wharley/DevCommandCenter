//! The strips over the prompt and the auto-resume toggle: a task stopped by
//! its plan's usage limit (`UsageLimitStrip`), the provider's own goal
//! (`GoalStrip`) and "Continuar sozinho se o limite acabar"
//! (`SessionAutoResumeToggle`).

use chrono::{DateTime, Local, Utc};
use dcc_core::domain::usage_limit::{UsageLimitPauseReason, UsageLimitPhase, usage_limit_phase};
use gpui::{AnyElement, App, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::Composer;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::data::goal::{
    GoalAction, GoalTone, clear_prompt, format_tokens, present, resume_prompt,
};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonVariant, Icon, Toaster, Tooltip, spinner};

/// A strip action on its way to the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripAction {
    Resume,
    GoalResume,
    GoalClear,
    GoalDismiss,
    AutoResume,
}

/// `formatLimitReset`: the time today, the date before it otherwise (pt-BR).
pub fn format_reset(resets_at: Option<&str>, now: DateTime<Utc>) -> Option<String> {
    let at = DateTime::parse_from_rfc3339(resets_at?)
        .ok()?
        .with_timezone(&Local);
    let today = now.with_timezone(&Local).date_naive();
    let time = at.format("%H:%M").to_string();
    if at.date_naive() == today {
        Some(time)
    } else {
        Some(format!("{} {time}", at.format("%d/%m")))
    }
}

/// `mb-2 rounded-xl border px-3 py-2 text-[12px]` in the strip's tone.
fn strip(border: Hsla, background: Hsla) -> gpui::Div {
    div()
        .mb(px(8.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(12.0))
        .line_height(px(16.0))
}

/// `Button size="sm" className="h-7 gap-1 px-2 text-[11px]"`.
fn strip_button(
    id: &str,
    variant: ButtonVariant,
    busy: bool,
    icon: &'static str,
    label: SharedString,
    disabled: bool,
) -> Button {
    Button::new(SharedString::from(id.to_string()))
        .variant(variant)
        .disabled(disabled)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_size(px(11.0))
                .child(if busy {
                    spinner(SharedString::from(format!("{id}-busy")), 12.0).into_any_element()
                } else {
                    Icon::new(if icon == "play" { "play-filled" } else { icon })
                        .size(12.0)
                        .into_any_element()
                })
                .child(label),
        )
}

pub(super) fn usage_limit_strip(
    composer: &Composer,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    let limit = composer.session.usage_limit.as_ref()?;
    let now = Utc::now();
    let status = usage_limit_phase(limit, now);
    let waiting = status.phase == UsageLimitPhase::Waiting;
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let reset_passed = limit
        .resets_at
        .as_deref()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .is_some_and(|at| at.with_timezone(&Utc) <= now);
    let time = format_reset(limit.resets_at.as_deref(), now);
    let when = if reset_passed {
        cx.t("usageLimit.whenSoon")
    } else {
        cx.t("usageLimit.whenBack")
    };
    let body = if waiting {
        if limit.auto_resume {
            cx.t_with(
                "usageLimit.waitingAuto",
                &Args::default().with("when", when.to_string()),
            )
        } else {
            cx.t("usageLimit.waitingManual")
        }
    } else {
        match status.pause_reason {
            Some(UsageLimitPauseReason::ResetPassed) => cx.t("usageLimit.reasonResetPassed"),
            Some(UsageLimitPauseReason::AttemptsExhausted) => {
                cx.t("usageLimit.reasonAttemptsExhausted")
            }
            _ => cx.t("usageLimit.reasonUnknownReset"),
        }
    };
    let action = if waiting {
        cx.t("usageLimit.continueNow")
    } else if status.pause_reason == Some(UsageLimitPauseReason::ResetPassed) {
        cx.t("usageLimit.continue")
    } else {
        cx.t("usageLimit.retry")
    };
    let (border, background, icon) = if waiting {
        (
            tw::VIOLET_500.alpha(0.3),
            tw::VIOLET_500.alpha(0.07),
            if dark {
                tw::VIOLET_300.hsla()
            } else {
                tw::VIOLET_600.hsla()
            },
        )
    } else {
        (
            tw::AMBER_500.alpha(0.35),
            tw::AMBER_500.alpha(0.1),
            if dark {
                tw::AMBER_300.hsla()
            } else {
                tw::AMBER_600.hsla()
            },
        )
    };
    let suffix = (waiting && !reset_passed)
        .then_some(time)
        .flatten()
        .map(|time| {
            let key = if limit.auto_resume {
                "usageLimit.backAt"
            } else {
                "usageLimit.freesAt"
            };
            format!(" · {}", cx.t_with(key, &Args::default().with("time", time)))
        });
    let busy = composer.strip_busy == Some(StripAction::Resume);
    let disabled = composer.session.running || composer.strip_busy.is_some();
    // Continuing on another account needs the accounts and a desktop route
    // (`resumeAfterUsageLimitOnAccount`): a later slice.
    Some(
        strip(border, background)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(Icon::new("hourglass").size(14.0).color(icon)),
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
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(if waiting {
                                        cx.t("usageLimit.waitingTitle")
                                    } else {
                                        cx.t("usageLimit.pausedTitle")
                                    })
                                    .children(suffix.map(|suffix| {
                                        div()
                                            .font_weight(FontWeight::NORMAL)
                                            .text_color(colors.muted_foreground)
                                            .child(suffix)
                                    })),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_color(colors.muted_foreground)
                                    .child(body),
                            ),
                    )
                    .child(
                        strip_button(
                            "usage-limit-resume",
                            if waiting {
                                ButtonVariant::Ghost
                            } else {
                                ButtonVariant::Outline
                            },
                            busy,
                            "play",
                            action,
                            disabled,
                        )
                        .on_click(
                            cx.listener(|composer, _, _, cx| composer.resume_after_limit(cx)),
                        ),
                    ),
            )
            .into_any_element(),
    )
}

pub(super) fn goal_strip(composer: &Composer, cx: &mut Context<Composer>) -> Option<AnyElement> {
    let goal = composer.session.goal.as_ref()?;
    let working = composer.session.running;
    let presented = present(goal, working);
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let tint =
        |light: tw::Oklch, dark_tone: tw::Oklch| if dark { dark_tone.hsla() } else { light.hsla() };
    let (border, background, icon_color) = match presented.tone {
        GoalTone::Done => (
            tw::EMERALD_500.alpha(0.3),
            tw::EMERALD_500.alpha(0.07),
            tint(tw::EMERALD_600, tw::EMERALD_300),
        ),
        GoalTone::Attention => (
            tw::AMBER_500.alpha(0.35),
            tw::AMBER_500.alpha(0.1),
            tint(tw::AMBER_600, tw::AMBER_300),
        ),
        GoalTone::Active => (
            tw::SKY_500.alpha(0.3),
            tw::SKY_500.alpha(0.06),
            tint(tw::SKY_600, tw::SKY_300),
        ),
    };
    let mut usage: Vec<String> = Vec::new();
    if presented.checks > 0 {
        usage.push(
            cx.t_with("goal.checks", &Args::count(presented.checks as i64))
                .to_string(),
        );
    }
    if let Some(used) = presented.tokens_used {
        usage.push(match presented.token_budget {
            Some(budget) => cx
                .t_with(
                    "goal.tokensOfBudget",
                    &Args::default()
                        .with("used", format_tokens(used))
                        .with("budget", format_tokens(budget)),
                )
                .to_string(),
            None => cx
                .t_with(
                    "goal.tokens",
                    &Args::default().with("used", format_tokens(used)),
                )
                .to_string(),
        });
    }
    if let Some(minutes) = presented.minutes {
        usage.push(
            cx.t_with("goal.minutes", &Args::count(minutes as i64))
                .to_string(),
        );
    }
    let pending = composer.strip_busy;
    let disabled = pending.is_some();
    let icon_name = if presented.tone == GoalTone::Done {
        "circle-check"
    } else {
        "target"
    };
    let buttons = (!working).then(|| {
        let mut row = div().flex().flex_shrink_0().items_center().gap(px(4.0));
        if presented.action == GoalAction::ResumeAndClear {
            row = row.child(
                strip_button(
                    "goal-resume",
                    ButtonVariant::Outline,
                    pending == Some(StripAction::GoalResume),
                    "play",
                    cx.t("goal.resume"),
                    disabled,
                )
                .on_click(cx.listener(|composer, _, _, cx| {
                    composer.goal_action(StripAction::GoalResume, cx)
                })),
            );
        }
        if presented.action == GoalAction::Dismiss {
            row = row.child(
                strip_button(
                    "goal-dismiss",
                    ButtonVariant::Ghost,
                    pending == Some(StripAction::GoalDismiss),
                    "x",
                    cx.t("goal.dismiss"),
                    disabled,
                )
                .on_click(cx.listener(|composer, _, _, cx| {
                    composer.goal_action(StripAction::GoalDismiss, cx)
                })),
            );
        } else {
            row = row.child(Tooltip::text(
                "goal-clear-tip",
                strip_button(
                    "goal-clear",
                    ButtonVariant::Ghost,
                    pending == Some(StripAction::GoalClear),
                    "x",
                    cx.t("goal.clear"),
                    disabled,
                )
                .on_click(cx.listener(|composer, _, _, cx| {
                    composer.goal_action(StripAction::GoalClear, cx)
                })),
                cx.t("goal.clearHint"),
            ));
        }
        row
    });
    Some(
        strip(border, background)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .mt(px(2.0))
                            .child(Icon::new(icon_name).size(14.0).color(icon_color)),
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
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(cx.t(presented.title_key))
                                    .when(!usage.is_empty(), |title| {
                                        title.child(
                                            div()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(colors.muted_foreground)
                                                .child(format!(" · {}", usage.join(" · "))),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_color(colors.muted_foreground)
                                    .line_clamp(2)
                                    .child(SharedString::from(goal.objective.clone())),
                            )
                            .children(presented.last_check.map(|reason| {
                                div()
                                    .mt(px(4.0))
                                    .text_size(px(11.5))
                                    .text_color(colors.muted_foreground.opacity(0.8))
                                    .line_clamp(2)
                                    .child(cx.t_with(
                                        "goal.lastCheck",
                                        &Args::default().with("reason", reason),
                                    ))
                            }))
                            .when(
                                !working && presented.action == GoalAction::Clear,
                                |column| {
                                    column.child(
                                        div()
                                            .mt(px(4.0))
                                            .text_size(px(11.5))
                                            .text_color(colors.muted_foreground.opacity(0.8))
                                            .child(cx.t("goal.idleHint")),
                                    )
                                },
                            ),
                    )
                    .children(buttons),
            )
            .into_any_element(),
    )
}

pub(super) fn auto_resume_toggle(
    composer: &Composer,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    composer.session.session_id.as_ref()?;
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let enabled = composer.session.auto_resume;
    let color = if enabled {
        if dark {
            tw::VIOLET_300.hsla()
        } else {
            tw::VIOLET_600.hsla()
        }
    } else {
        colors.muted_foreground.opacity(0.7)
    };
    let hover = colors.accent.opacity(0.6);
    let saving = composer.strip_busy == Some(StripAction::AutoResume);
    let hint = format!(
        "{}\n{}",
        cx.t("usageLimit.toggleLabel"),
        cx.t(if enabled {
            "usageLimit.toggleOn"
        } else {
            "usageLimit.toggleOff"
        })
    );
    Some(
        Tooltip::text(
            "auto-resume-tip",
            div()
                .id("auto-resume")
                .flex()
                .h(px(28.0))
                .flex_shrink_0()
                .items_center()
                .px(px(6.0))
                .rounded(px(9.0))
                .text_color(color)
                .when(!saving, |button| {
                    button
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(|composer, _, _, cx| composer.toggle_auto_resume(cx)))
                })
                .when(saving, |button| button.opacity(0.45))
                .child(Icon::new("hourglass").size(13.0).stroke(1.8)),
            hint,
        )
        .into_any_element(),
    )
}

impl Composer {
    fn run_strip(
        &mut self,
        action: StripAction,
        job: impl FnOnce(&DesktopApi) -> Result<(), ApiError> + Send + 'static,
        failure: impl Fn(&str, &App) -> SharedString + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.strip_busy.is_some() {
            return;
        }
        self.strip_busy = Some(action);
        let endpoint = self.endpoint.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?;
                    job(&api)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.strip_busy = None;
                if result.is_err() && action == StripAction::AutoResume {
                    // The optimistic flip goes back.
                    this.session.auto_resume = !this.session.auto_resume;
                }
                match result {
                    Ok(()) => cx.emit(super::ComposerEvent::Refresh),
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => {
                        let message = failure(&error.to_string(), cx);
                        Toaster::error(message, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn resume_after_limit(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        self.run_strip(
            StripAction::Resume,
            move |api| api.resume_after_usage_limit(&session),
            |message, cx| {
                cx.t_with(
                    "usageLimit.resumeFailed",
                    &Args::default().with("message", message.to_string()),
                )
            },
            cx,
        );
    }

    pub(crate) fn toggle_auto_resume(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let enabled = !self.session.auto_resume;
        self.session.auto_resume = enabled;
        self.run_strip(
            StripAction::AutoResume,
            move |api| api.set_auto_resume(&session, enabled),
            |message, cx| {
                cx.t_with(
                    "usageLimit.toggleFailed",
                    &Args::default().with("message", message.to_string()),
                )
            },
            cx,
        );
    }

    pub(crate) fn goal_action(&mut self, action: StripAction, cx: &mut Context<Self>) {
        let provider = self.session.provider_id.clone();
        match action {
            StripAction::GoalResume => self.send_text(resume_prompt(provider.as_deref()), cx),
            StripAction::GoalClear => self.send_text(clear_prompt(provider.as_deref()), cx),
            StripAction::GoalDismiss => {
                let Some(session) = self.session.session_id.clone() else {
                    return;
                };
                self.run_strip(
                    StripAction::GoalDismiss,
                    move |api| api.dismiss_goal(&session),
                    |message, cx| {
                        cx.t_with(
                            "goal.actionFailed",
                            &Args::default().with("message", message.to_string()),
                        )
                    },
                    cx,
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_times_read_like_the_desktop() {
        let noon = Local::now().date_naive().and_hms_opt(12, 0, 0).unwrap();
        let now = noon.and_local_timezone(Local).unwrap().with_timezone(&Utc);
        let later = (now + chrono::Duration::minutes(5)).to_rfc3339();
        let today = format_reset(Some(&later), now).unwrap();
        assert_eq!(today.len(), 5, "{today}");
        let next_week = (now + chrono::Duration::days(7)).to_rfc3339();
        assert!(format_reset(Some(&next_week), now).unwrap().contains('/'));
        assert_eq!(format_reset(None, now), None);
    }
}
