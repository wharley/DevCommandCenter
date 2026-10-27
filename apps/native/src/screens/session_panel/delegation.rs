//! One delegation's card (`DelegationsSection`): its status, mode and
//! provider, the report, the files it touched and its actions — open the
//! child, review the diff, apply, discard, mark reviewed.

use gpui::{AnyElement, Context, FontWeight, Hsla, SharedString, div, prelude::*, px};

use super::SessionPanel;
use super::activity::provider;
use crate::data::conversation::DelegationRecord;
use crate::data::session_activity::status_key;
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Badge, BadgeVariant, Button, ButtonSize, ButtonVariant, Tracked, spinner};

/// `delegationStatusClass`: border, background and text.
fn status_colors(status: &str, cx: &gpui::App) -> (Hsla, Hsla, Hsla) {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    let tinted = |base: tw::Oklch, light: tw::Oklch, night: tw::Oklch| {
        let text = if dark { night.hsla() } else { light.hsla() };
        (base.hsla().opacity(0.3), base.hsla().opacity(0.1), text)
    };
    match status {
        "completed" => tinted(tw::EMERALD_500, tw::EMERALD_700, tw::EMERALD_300),
        "failed" => (
            colors.destructive.opacity(0.3),
            colors.destructive.opacity(0.1),
            colors.destructive,
        ),
        "cancelled" => (
            colors.muted_foreground.opacity(0.3),
            colors.muted.opacity(0.3),
            colors.muted_foreground,
        ),
        "running" => tinted(tw::SKY_500, tw::SKY_700, tw::SKY_300),
        "review_pending" => tinted(tw::AMBER_500, tw::AMBER_700, tw::AMBER_300),
        _ => (
            colors.border,
            colors.muted.opacity(0.3),
            colors.muted_foreground,
        ),
    }
}

fn small_button(id: String, label: SharedString, variant: ButtonVariant, disabled: bool) -> Button {
    Button::new(SharedString::from(id))
        .variant(variant)
        .size(ButtonSize::Xs)
        .height(24.0)
        .padding_x(8.0)
        .text_size(11.0)
        .disabled(disabled)
        .child(label)
}

impl SessionPanel {
    pub(super) fn delegation_card(
        &self,
        delegation: &DelegationRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let status = status_key(delegation);
        let (border, background, text) = status_colors(&status, cx);
        let busy = self.busy.is_some();
        let id = delegation.id.clone();
        let body = delegation
            .result_summary
            .clone()
            .or_else(|| delegation.diff_summary.clone())
            .unwrap_or_else(|| delegation.prompt.clone());
        let mono = cx.theme().fonts.mono.clone();
        let pending = status == "review_pending";
        let files = delegation
            .touched_files
            .iter()
            .take(8)
            .enumerate()
            .map(|(index, path)| {
                let record = delegation.clone();
                let target = path.clone();
                Button::new(SharedString::from(format!(
                    "session-delegation-file-{id}-{index}"
                )))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Xs)
                .height(24.0)
                .padding_x(6.0)
                .text_size(10.5)
                .on_click(
                    cx.listener(move |panel, _, _, cx| panel.review_file(&record, &target, cx)),
                )
                .child(
                    div()
                        .max_w(px(224.0))
                        .truncate()
                        .font_family(mono.clone())
                        .child(path.clone()),
                )
            })
            .collect::<Vec<_>>();
        let more = delegation.touched_files.len().saturating_sub(8);
        let action = |name: &str, label: &str, variant, cx: &mut Context<Self>| {
            let record = delegation.clone();
            let kind = name.to_string();
            let running = self.busy.as_deref() == Some(format!("{name}:{id}").as_str());
            small_button(
                format!("session-delegation-{name}-{id}"),
                cx.t(label),
                variant,
                busy,
            )
            .on_click(cx.listener(move |panel, _, _, cx| match kind.as_str() {
                "apply" => panel.apply(&record, cx),
                "discard" => panel.discard(&record, cx),
                "approve" => panel.approve(&record, cx),
                "child" => {
                    if let Some(child) = record.child_session_id.clone() {
                        cx.emit(super::SessionPanelEvent::OpenSession(child));
                    }
                }
                _ => {
                    if let Some(first) = record.touched_files.first().cloned() {
                        panel.review_file(&record, &first, cx);
                    }
                }
            }))
            .children(
                running.then(|| spinner(SharedString::from(format!("session-busy-{name}")), 12.0)),
            )
        };
        let mut actions: Vec<AnyElement> = Vec::new();
        if delegation.child_session_id.is_some() {
            actions.push(
                action(
                    "child",
                    "inspector.delegations.openChild",
                    ButtonVariant::Outline,
                    cx,
                )
                .into_any_element(),
            );
        }
        if !delegation.touched_files.is_empty() {
            actions.push(
                action(
                    "review",
                    "inspector.delegations.reviewDiff",
                    ButtonVariant::Ghost,
                    cx,
                )
                .into_any_element(),
            );
        }
        if pending && delegation.implement {
            actions.push(
                action(
                    "apply",
                    "inspector.delegations.applyChanges",
                    ButtonVariant::Default,
                    cx,
                )
                .into_any_element(),
            );
            actions.push(
                action(
                    "discard",
                    "inspector.delegations.discardChanges",
                    ButtonVariant::Destructive,
                    cx,
                )
                .into_any_element(),
            );
        }
        if pending {
            let variant = if delegation.implement {
                ButtonVariant::Outline
            } else {
                ButtonVariant::Default
            };
            actions.push(
                action("approve", "inspector.delegations.markReviewed", variant, cx)
                    .into_any_element(),
            );
        }
        div()
            .debug_selector({
                let id = id.clone();
                move || format!("session-delegation-{id}")
            })
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border.opacity(0.5))
            .bg(colors.muted.opacity(0.1))
            .px(px(8.0))
            .py(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(4.0))
                            .h(px(20.0))
                            .px(px(6.0))
                            .rounded(px(26.0))
                            .border_1()
                            .border_color(border)
                            .bg(background)
                            .text_size(px(10.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(text)
                            .when(status == "running", |badge| {
                                badge.child(div().size(px(6.0)).rounded_full().bg(text))
                            })
                            .child(cx.t(&format!("inspector.delegations.status.{status}"))),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!(
                                "{} · {}",
                                cx.t(&format!("inspector.delegations.mode.{}", delegation.mode)),
                                provider(&delegation.provider_id)
                            )),
                    ),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .line_clamp(2)
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(colors.muted_foreground)
                    .child(body),
            )
            .children(delegation.diff_summary.clone().map(|summary| {
                div()
                    .mt(px(4.0))
                    .font_family(cx.theme().fonts.mono.clone())
                    .text_size(px(10.5))
                    .text_color(colors.muted_foreground)
                    .child(summary)
            }))
            .children(delegation.validation_summary.clone().map(|summary| {
                div()
                    .mt(px(4.0))
                    .line_clamp(2)
                    .text_size(px(10.5))
                    .text_color(colors.muted_foreground)
                    .child(summary)
            }))
            .when(!delegation.touched_files.is_empty(), |card| {
                card.child(
                    div()
                        .mt(px(8.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(colors.muted_foreground)
                                .child(Tracked::new(
                                    cx.t_with(
                                        "inspector.delegations.files",
                                        &Args::count(delegation.touched_files.len() as i64),
                                    )
                                    .to_uppercase(),
                                    10.0,
                                    0.12,
                                )),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .flex()
                                .flex_wrap()
                                .gap(px(4.0))
                                .children(files)
                                .when(more > 0, |row| {
                                    row.child(
                                        Badge::new(format!("+{more}"), BadgeVariant::Outline)
                                            .text_size(10.0),
                                    )
                                }),
                        ),
                )
            })
            .when(!actions.is_empty(), |card| {
                card.child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .children(actions),
                )
            })
            .into_any_element()
    }
}
