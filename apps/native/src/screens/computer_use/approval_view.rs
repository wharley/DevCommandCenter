//! The card as the desktop draws it (`w-[min(92vw,560px)]`, at most
//! `min(88vh,760px)` tall, its body scrolling).

use gpui::{
    AnyElement, Context, FontWeight, IntoElement, Render, SharedString, Window, div, prelude::*, px,
};

use super::approval::{ComputerApprovals, format_remaining};
use crate::data::computer_use::AccessKind;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::dialog::{Dialog, dialog_footer};
use crate::ui::{Button, ButtonSize, ButtonVariant, Checkbox, Icon};

/// `shortId`: long ids as `12345678…cdef`.
fn short_id(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() > 12 {
        let head: String = chars[..8].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    } else {
        value.to_string()
    }
}

fn small(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .child(text.into())
}

impl ComputerApprovals {
    fn apps(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let busy = self.busy.is_some();
        let chosen = self.chosen_apps();
        let apps = self
            .status
            .as_ref()
            .map(|status| status.apps())
            .unwrap_or_default();
        let list = if apps.is_empty() {
            div()
                .rounded(px(4.0))
                .border_1()
                .border_dashed()
                .border_color(colors.border)
                .p(px(12.0))
                .text_color(colors.muted_foreground)
                .child(small(if self.status.is_none() && !self.status_failed {
                    cx.t("settings.computerUse.approval.loadingStatus")
                } else {
                    cx.t("settings.computerUse.approval.appsEmpty")
                }))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .children(apps.into_iter().enumerate().map(|(index, app)| {
                    let checked = chosen.contains(&app.bundle_id);
                    let bundle = app.bundle_id.clone();
                    div()
                        .id(("computer-app", index))
                        .debug_selector(move || format!("computer-app-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .rounded(px(4.0))
                        .px(px(8.0))
                        .py(px(6.0))
                        .when(!busy, |row| {
                            row.cursor_pointer()
                                .hover(|row| row.bg(colors.muted.opacity(0.5)))
                                .on_click(cx.listener(move |approvals, _, _, cx| {
                                    approvals.toggle_app(&bundle, cx)
                                }))
                        })
                        .child(Checkbox::new(("computer-app-check", index), checked).disabled(busy))
                        .child(app.name)
                }))
                .into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(cx.t("settings.computerUse.approval.appsTitle")),
                    )
                    .child(
                        small(cx.t("settings.computerUse.approval.appsHint"))
                            .text_color(colors.muted_foreground),
                    )
                    .child(
                        small(cx.t("settings.computerUse.approval.inputHint"))
                            .mt(px(4.0))
                            .text_color(colors.muted_foreground),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    /// The two macOS permissions still missing, each with its settings.
    fn permissions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let status = self.status.as_ref()?;
        if status.permissions_ready() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let busy = self.busy.is_some() || self.busy_access.is_some();
        let buttons: Vec<AnyElement> = [
            (
                AccessKind::Accessibility,
                status.accessibility,
                "openAccessibility",
            ),
            (
                AccessKind::ScreenRecording,
                status.screen_recording,
                "openScreenRecording",
            ),
        ]
        .into_iter()
        .filter(|(_, permission, _)| !permission.granted)
        .map(|(kind, permission, key)| {
            let label = if self.busy_access == Some(kind) {
                cx.t("settings.computerUse.loading")
            } else {
                cx.t(&format!("settings.computerUse.approval.{key}"))
            };
            Button::new(SharedString::from(format!("computer-access-{key}")))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(!permission.can_request || busy)
                .on_click(cx.listener(move |approvals, _, _, cx| approvals.open_access(kind, cx)))
                .child(label)
                .into_any_element()
        })
        .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .p(px(12.0))
                .child(
                    small(cx.t("settings.computerUse.approval.permissions"))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(div().flex().flex_wrap().gap(px(8.0)).children(buttons))
                .into_any_element(),
        )
    }
}

impl Render for ComputerApprovals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(waiting) = self.current().cloned() else {
            return div().into_any_element();
        };
        let remaining = waiting.remaining_ms();
        if remaining == 0 {
            return div().into_any_element();
        }
        let request = waiting.request;
        let colors = cx.theme().colors.clone();
        let muted = colors.muted_foreground;
        let title = (self.title_of)(
            &crate::data::browser_opens::OpenRequest {
                workspace_id: request.workspace_id.clone(),
                session_id: request.session_id.clone(),
                ..Default::default()
            },
            cx,
        );
        let known = title.is_some();
        let conversation = title.unwrap_or_else(|| {
            cx.t_with(
                "settings.computerUse.approval.unknownConversation",
                &Args::default().with("id", short_id(&request.session_id)),
            )
            .to_string()
        });
        let mut labels = vec![
            cx.t("settings.computerUse.approval.provider"),
            cx.t("settings.computerUse.approval.conversation"),
        ];
        let mut values = vec![request.provider_id.clone(), conversation];
        if !known {
            labels.push(cx.t("settings.computerUse.approval.workspace"));
            values.push(
                cx.t_with(
                    "settings.computerUse.approval.unknownWorkspace",
                    &Args::default().with("id", short_id(&request.workspace_id)),
                )
                .to_string(),
            );
        }
        let details = div()
            .flex()
            .gap(px(12.0))
            .text_size(px(12.0))
            .line_height(px(16.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .text_color(muted)
                    .children(labels),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .min_w_0()
                    .font_weight(FontWeight::MEDIUM)
                    .children(values),
            );
        let reason = div()
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.muted.opacity(0.3))
            .p(px(12.0))
            .child(
                small(cx.t("settings.computerUse.approval.reason"))
                    .mb(px(4.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted),
            )
            .child(div().child(request.reason.clone()));
        let clock = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_color(muted)
            .child(Icon::new("timer").size(14.0))
            .child(small(cx.t_with(
                "settings.computerUse.approval.remaining",
                &Args::default().with("time", format_remaining(remaining)),
            )));
        let notes: Vec<AnyElement> = [
            self.status_failed
                .then(|| cx.t("settings.computerUse.approval.statusError")),
            self.status
                .as_ref()
                .filter(|status| !status.supported)
                .map(|_| cx.t("settings.computerUse.approval.unsupported")),
            self.status
                .as_ref()
                .filter(|status| status.supported && !status.runtime_attached)
                .map(|_| cx.t("settings.computerUse.approval.conversationNotReady")),
        ]
        .into_iter()
        .flatten()
        .map(|note| small(note).text_color(muted).into_any_element())
        .chain(
            [self.access_error.clone(), self.decision_error.clone()]
                .into_iter()
                .flatten()
                .map(|error| {
                    small(error)
                        .text_color(colors.destructive)
                        .into_any_element()
                }),
        )
        .collect();
        let body = div()
            .id("computer-approval-body")
            .debug_selector(|| "computer-approval".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(16.0))
            .overflow_y_scroll()
            .pr(px(4.0))
            .child(details)
            .child(reason)
            .child(clock)
            .child(self.apps(cx))
            .children(self.permissions(cx))
            .children(notes);
        let busy = self.busy.is_some();
        let deny = Button::new("computer-approval-deny")
            .variant(ButtonVariant::Outline)
            .disabled(busy)
            .on_click(cx.listener(|approvals, _, _, cx| approvals.decide(false, cx)))
            .child(cx.t("settings.computerUse.approval.deny"));
        let allow = Button::new("computer-approval-allow")
            .disabled(!self.can_allow())
            .on_click(cx.listener(|approvals, _, _, cx| approvals.decide(true, cx)))
            .child(cx.t("settings.computerUse.approval.allow"));
        let header = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(16.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(Icon::new("shield-alert").size(16.0))
                    .child(cx.t("settings.computerUse.approval.title")),
            )
            .child(div().text_color(muted).child(format!(
                "{} {}",
                cx.t("settings.computerUse.approval.description"),
                cx.t("settings.computerUse.approval.taskWide")
            )));
        Dialog::new("computer-approval", |_, _| {})
            .max_width(560.0)
            .max_height((f32::from(window.viewport_size().height) * 0.88).min(760.0))
            .close_button(false)
            .child(header)
            .child(body)
            .child(dialog_footer(
                [deny.into_any_element(), allow.into_any_element()],
                cx,
            ))
            .into_any_element()
    }
}
