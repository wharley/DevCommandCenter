//! A check in the summary's card, beyond its name: its page on a click,
//! and for a failed GitHub Actions job the failing step's log under it and
//! "Rodar de novo". The log is read when asked for, once per job; nothing
//! is read ahead and nothing re-runs without the click.

use std::collections::HashMap;

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::PrHub;
use crate::data::desktop_api::ApiError;
use crate::data::pr_checks::is_actions_job;
use crate::data::pr_hub::Check;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Toaster, Tooltip, spinner};

/// A job's log, once asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Log {
    Reading,
    Read(String),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct CheckLogs {
    /// The check whose log shows, by its link.
    pub open: Option<String>,
    pub logs: HashMap<String, Log>,
    /// The job being run again.
    pub rerunning: Option<String>,
}

fn reason(error: ApiError, cx: &gpui::App) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        other => other.text(cx),
    }
}

impl PrHub {
    pub(crate) fn toggle_check_log(&mut self, link: String, cx: &mut Context<Self>) {
        if self.checks.open.as_deref() == Some(link.as_str()) {
            self.checks.open = None;
            cx.notify();
            return;
        }
        self.checks.open = Some(link.clone());
        cx.notify();
        if matches!(
            self.checks.logs.get(&link),
            Some(Log::Reading | Log::Read(_))
        ) {
            return;
        }
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        self.checks.logs.insert(link.clone(), Log::Reading);
        cx.spawn(async move |this, cx| {
            let asked = link.clone();
            let read = cx
                .background_executor()
                .spawn(async move { api.pr_hub_check_log(&item, &asked) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                let log = match read {
                    Ok(excerpt) => Log::Read(excerpt),
                    Err(error) => Log::Failed(reason(error, cx)),
                };
                hub.checks.logs.insert(link, log);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn rerun_check(&mut self, link: String, cx: &mut Context<Self>) {
        let (Some(api), Some(item)) = (self.api.clone(), self.selected_item(cx)) else {
            return;
        };
        if self.checks.rerunning.is_some() {
            return;
        }
        self.checks.rerunning = Some(link.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = link.clone();
            let done = cx
                .background_executor()
                .spawn(async move { api.pr_hub_check_rerun(&item, &asked) })
                .await;
            let _ = this.update(cx, |hub, cx| {
                hub.checks.rerunning = None;
                match done {
                    Ok(()) => {
                        Toaster::success(cx.t("pullRequests.checkActions.rerunDone"), cx);
                        // The old log is of the run that failed.
                        hub.checks.logs.remove(&link);
                        hub.checks.open = None;
                        hub.detail = None;
                        hub.ensure_detail(cx);
                    }
                    Err(error) => Toaster::error(reason(error, cx), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The row's buttons: its page; for a failed Actions job, the log and
    /// the re-run.
    pub(super) fn check_buttons(
        &self,
        index: usize,
        check: &Check,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(link) = check.details_url.clone().filter(|link| !link.is_empty()) else {
            return Vec::new();
        };
        let colors = cx.theme().colors.clone();
        let hover = colors.muted.opacity(0.6);
        let button = move |id: SharedString, on: bool| {
            div()
                .id(id)
                .flex()
                .flex_none()
                .size(px(22.0))
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .when(on, |button| button.bg(hover))
                .hover(move |button| button.bg(hover))
        };
        let mut buttons = Vec::new();
        if check.state == "failure" && is_actions_job(&link) {
            let open = self.checks.open.as_deref() == Some(link.as_str());
            let (log_link, rerun_link) = (link.clone(), link.clone());
            buttons.push(
                Tooltip::text(
                    ("pr-check-log-tip", index),
                    button(format!("pr-check-log-{index}").into(), open)
                        .debug_selector(move || format!("pr-check-log-{index}"))
                        .on_click(cx.listener(move |hub, _, _, cx| {
                            hub.toggle_check_log(log_link.clone(), cx)
                        }))
                        .child(Icon::new("file-text").size(13.0).stroke(1.8)),
                    cx.t("pullRequests.checkActions.log"),
                )
                .into_any_element(),
            );
            let rerunning = self.checks.rerunning.as_deref() == Some(link.as_str());
            buttons.push(
                Tooltip::text(
                    ("pr-check-rerun-tip", index),
                    button(format!("pr-check-rerun-{index}").into(), false)
                        .debug_selector(move || format!("pr-check-rerun-{index}"))
                        .on_click(
                            cx.listener(move |hub, _, _, cx| {
                                hub.rerun_check(rerun_link.clone(), cx)
                            }),
                        )
                        .child(if rerunning {
                            spinner(("pr-check-rerunning", index), 13.0).into_any_element()
                        } else {
                            Icon::new("rotate-ccw")
                                .size(13.0)
                                .stroke(1.8)
                                .into_any_element()
                        }),
                    cx.t("pullRequests.checkActions.rerun"),
                )
                .into_any_element(),
            );
        }
        buttons.push(
            Tooltip::text(
                ("pr-check-open-tip", index),
                button(format!("pr-check-open-{index}").into(), false)
                    .debug_selector(move || format!("pr-check-open-{index}"))
                    .on_click(move |_, _, cx| cx.open_url(&link))
                    .child(Icon::new("arrow-up-right").size(13.0).stroke(1.8)),
                cx.t("pullRequests.checkActions.open"),
            )
            .into_any_element(),
        );
        buttons
    }

    /// Under the row: the failing step of the check whose log is open.
    pub(super) fn check_log(&self, check: &Check, cx: &Context<Self>) -> Option<AnyElement> {
        let link = check.details_url.as_deref()?;
        if self.checks.open.as_deref() != Some(link) {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let quiet = |text: SharedString| {
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(text)
                .into_any_element()
        };
        let body = match self.checks.logs.get(link)? {
            Log::Reading => spinner("pr-check-log-reading", 14.0).into_any_element(),
            Log::Failed(error) => quiet(error.clone().into()),
            Log::Read(excerpt) if excerpt.trim().is_empty() => {
                quiet(cx.t("pullRequests.checkActions.noLog"))
            }
            Log::Read(excerpt) => div()
                .font_family(cx.theme().fonts.mono.clone())
                .text_size(px(11.0))
                .line_height(px(16.0))
                .children(excerpt.lines().map(|line| {
                    div()
                        .min_w_0()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(SharedString::from(line.to_string()))
                }))
                .into_any_element(),
        };
        Some(
            div()
                .debug_selector(|| "pr-check-log".into())
                .ml(px(21.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.muted.opacity(0.3))
                .p(px(8.0))
                .overflow_hidden()
                .child(body)
                .into_any_element(),
        )
    }
}
