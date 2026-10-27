//! "Computer Use" (`ComputerUsePanel`): what the Browser does instead, the
//! Mac's support, which providers can use it, the two macOS permissions
//! (with a button that opens their settings), and while a conversation's
//! grant is armed, its countdown and "Desativar agora". Read again every 5s
//! while the section shows; the countdown ticks every second.

use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, FontWeight, SharedString, Task, div, prelude::*, px};

use super::{SectionId, Settings};
use crate::data::computer_use::{AccessKind, ComputerStatus};
use crate::data::desktop_api::ApiError;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};
use dcc_core::domain::provider::McpSupportLevel;

mod cards;

const POLL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(super) struct Computer {
    /// The last status read, and when.
    pub(super) status: Option<(ComputerStatus, Instant)>,
    /// The last read failed (with or without an older status).
    pub(super) failed: bool,
    pub(super) loading: bool,
    /// `accessibility`, `screenRecording` or `disarm` while it runs.
    pub(super) busy: Option<&'static str>,
    poll: Option<Task<()>>,
}

/// `formatRemaining`: `m:ss`.
pub(super) fn remaining(milliseconds: u64) -> String {
    let seconds = milliseconds.div_ceil(1000);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn mcp_label(support: &McpSupportLevel) -> &'static str {
    match support {
        McpSupportLevel::VerifiedBridge { .. } => "settings.computerUse.matrix.mcpVerified",
        McpSupportLevel::RuntimeBridge { .. } => "settings.computerUse.matrix.mcpRuntime",
        McpSupportLevel::NativeConfig => "settings.computerUse.matrix.mcpNative",
        McpSupportLevel::Unsupported => "settings.computerUse.matrix.mcpUnsupported",
    }
}

fn card(cx: &gpui::App) -> gpui::Div {
    div()
        .rounded(px(12.0))
        .border_1()
        .border_color(cx.theme().colors.border.opacity(0.6))
        .p(px(16.0))
}

fn heading(value: SharedString, cx: &gpui::App) -> gpui::Div {
    div()
        .text_size(px(14.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(value)
}

fn hint(size: f32, value: SharedString, cx: &gpui::App) -> gpui::Div {
    div()
        .text_size(px(size))
        .line_height(px(size * 1.625))
        .text_color(cx.theme().colors.muted_foreground)
        .child(value)
}

/// The square with the section's icon (`size-9 rounded-lg bg-primary/10`).
fn mark(icon: &'static str, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_none()
        .size(px(36.0))
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(colors.primary.opacity(0.1))
        .child(Icon::new(icon).size(16.0).color(colors.primary))
        .into_any_element()
}

impl Settings {
    /// Read when the section opens, then every 5s while it shows.
    pub(super) fn ensure_computer(&mut self, cx: &mut Context<Self>) {
        self.load_computer(cx);
        if self.computer.poll.is_some() {
            return;
        }
        self.computer.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let Ok(()) = this.update(cx, |settings, cx| {
                    if settings.active != SectionId::ComputerUse {
                        return;
                    }
                    let stale = settings
                        .computer
                        .status
                        .as_ref()
                        .is_none_or(|(_, at)| at.elapsed() >= POLL);
                    if stale && !settings.computer.loading {
                        settings.load_computer(cx);
                    }
                    // The countdown.
                    if settings
                        .computer
                        .status
                        .as_ref()
                        .is_some_and(|(status, _)| status.grant.armed)
                    {
                        cx.notify();
                    }
                }) else {
                    return;
                };
            }
        }));
    }

    pub(super) fn load_computer(&mut self, cx: &mut Context<Self>) {
        let Some(api) = self.general.api() else {
            self.computer.failed = true;
            cx.notify();
            return;
        };
        let session = self.session.session_id.clone();
        self.computer.loading = true;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { api.computer_settings(session.as_deref()) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                let computer = &mut settings.computer;
                computer.loading = false;
                match read {
                    Ok(status) => {
                        computer.status = Some((status, Instant::now()));
                        computer.failed = false;
                    }
                    Err(_) => computer.failed = true,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn computer_action(
        &mut self,
        busy: &'static str,
        action: impl FnOnce(crate::data::desktop_api::DesktopApi) -> Result<(), ApiError>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = self.general.api() else {
            return;
        };
        self.computer.busy = Some(busy);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { action(api) })
                .await;
            let _ = this.update(cx, |settings, cx| {
                settings.computer.busy = None;
                if let Err(error) = done {
                    let message = match error {
                        ApiError::Failed(message) if !message.trim().is_empty() => message,
                        _ => cx.t("settings.computerUse.errors.generic").to_string(),
                    };
                    Toaster::error(message, cx);
                }
                settings.load_computer(cx);
            });
        })
        .detach();
    }

    pub(super) fn request_computer_access(&mut self, kind: AccessKind, cx: &mut Context<Self>) {
        let busy = match kind {
            AccessKind::Accessibility => "accessibility",
            AccessKind::ScreenRecording => "screenRecording",
        };
        self.computer_action(busy, move |api| api.computer_access(kind).map(|_| ()), cx);
    }

    pub(super) fn disarm_computer(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        self.computer_action(
            "disarm",
            move |api| api.computer_disarm(&session).map(|_| ()),
            cx,
        );
    }

    fn browser_explanation(&self, cx: &gpui::App) -> AnyElement {
        let colors = &cx.theme().colors;
        card(cx)
            .bg(colors.muted.opacity(0.15))
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(mark("earth", cx))
            .child(
                div()
                    .min_w_0()
                    .child(heading(cx.t("settings.computerUse.browserTitle"), cx))
                    .child(hint(12.0, cx.t("settings.computerUse.browserHint"), cx).mt(px(4.0)))
                    .child(
                        hint(
                            12.0,
                            cx.t("settings.computerUse.browserPermissionsHint"),
                            cx,
                        )
                        .mt(px(8.0))
                        .text_color(colors.foreground),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn computer_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let column = div()
            .debug_selector(|| "settings-computer".into())
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.browser_explanation(cx));
        let Some((status, read_at)) = self.computer.status.clone() else {
            if !self.computer.failed {
                return column
                    .child(
                        card(cx)
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(12.0))
                            .text_color(colors.muted_foreground)
                            .child(spinner("settings-computer-loading", 14.0))
                            .child(cx.t("settings.computerUse.loading")),
                    )
                    .into_any_element();
            }
            return column
                .child(
                    div()
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(colors.destructive.opacity(0.3))
                        .bg(colors.destructive.opacity(0.05))
                        .p(px(16.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(colors.destructive)
                                .child(cx.t("settings.computerUse.errors.status")),
                        )
                        .child(
                            div().mt(px(12.0)).child(
                                Button::new("settings-computer-retry")
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .text_size(12.0)
                                    .on_click(
                                        cx.listener(|settings, _, _, cx| {
                                            settings.load_computer(cx)
                                        }),
                                    )
                                    .child(cx.t("settings.computerUse.retry")),
                            ),
                        ),
                )
                .into_any_element();
        };
        let left = if status.grant.armed {
            status
                .grant
                .remaining_ms
                .saturating_sub(read_at.elapsed().as_millis() as u64)
        } else {
            0
        };
        column
            .when(self.computer.failed, |column| {
                column.child(
                    div()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.destructive.opacity(0.3))
                        .bg(colors.destructive.opacity(0.05))
                        .px(px(12.0))
                        .py(px(8.0))
                        .text_size(px(12.0))
                        .text_color(colors.destructive)
                        .child(cx.t("settings.computerUse.errors.statusCached")),
                )
            })
            .child(self.computer_support(&status, cx))
            .child(self.computer_matrix(&status, cx))
            .child(self.computer_permissions(&status, cx))
            .child(
                card(cx)
                    .text_size(px(12.0))
                    .line_height(px(19.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("settings.computerUse.conversationHint")),
            )
            .when(left > 0, |column| {
                column.child(self.computer_grant(&status, left, cx))
            })
            .into_any_element()
    }
}
