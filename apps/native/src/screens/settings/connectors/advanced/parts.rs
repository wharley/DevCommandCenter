//! The panel's small pieces: its cards, text and badges.

use gpui::{AnyElement, FontWeight, Hsla, SharedString, div, prelude::*, px};

use crate::data::connectors::Integration;
use crate::data::mcp_advanced::{scope_key, scope_of};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Badge, BadgeVariant};

/// `rounded-xl border border-border/60 p-4` on a background.
pub(super) fn card(background: Option<Hsla>, cx: &gpui::App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .rounded(px(12.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .when_some(background, |card, background| card.bg(background))
        .p(px(16.0))
}

/// The amber card (`--warning` 45% into the border, 8% behind).
pub(super) fn warning_card(cx: &gpui::App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .rounded(px(12.0))
        .border_1()
        .border_color(mix_oklab(colors.warning, 0.45, colors.border))
        .bg(colors.warning.opacity(0.08))
        .p(px(16.0))
}

pub(super) fn text(
    size: f32,
    muted: bool,
    value: impl Into<SharedString>,
    cx: &gpui::App,
) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .text_size(px(size))
        .line_height(px(size * 1.625))
        .text_color(if muted {
            colors.muted_foreground
        } else {
            colors.foreground
        })
        .child(value.into())
}

pub(super) fn title(size: f32, value: impl Into<SharedString>, cx: &gpui::App) -> gpui::Div {
    div()
        .text_size(px(size))
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().colors.foreground)
        .child(value.into())
}

/// A button's icon and text (`gap-1.5`).
pub(super) fn labelled(icon: AnyElement, label: SharedString) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(icon)
        .child(label)
        .into_any_element()
}

fn transport_badge(integration: &Integration, cx: &gpui::App) -> Badge {
    Badge::new(
        cx.t(if integration.definition.transport.kind == "http" {
            "settings.integrations.http"
        } else {
            "settings.integrations.stdio"
        }),
        BadgeVariant::Outline,
    )
}

pub(super) fn badges(integration: &Integration, cx: &gpui::App) -> Vec<AnyElement> {
    let (scope, _) = scope_of(integration);
    vec![
        transport_badge(integration, cx).into_any_element(),
        Badge::new(cx.t(scope_key(scope)), BadgeVariant::Outline).into_any_element(),
    ]
}

pub(super) fn credential_badge(integration: &Integration, cx: &gpui::App) -> Option<AnyElement> {
    (integration.credential_count > 0).then(|| {
        Badge::new(
            cx.t_with(
                "settings.integrations.credentialCount",
                &Args::count(integration.credential_count as i64),
            ),
            BadgeVariant::Secondary,
        )
        .icon("key-round")
        .into_any_element()
    })
}
