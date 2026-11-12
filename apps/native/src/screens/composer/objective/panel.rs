//! The "Objetivo" trigger in the composer toolbar and its panel
//! (`session-objective.css`): header, the provider's goal or its two fields,
//! and the actions. Offered only where the provider has its own goal.

use dcc_core::domain::goal::GoalStatus;
use gpui::{AnyElement, App, Context, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use super::native;
use crate::i18n::Translate;
use crate::screens::composer::Composer;
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::theme::tailwind as tw;
use crate::ui::popover::{Popover, PopoverSide};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};

/// `.objective-panel`: `min(520px, 100vw - 24px)`.
const PANEL_WIDTH: f32 = 520.0;
const PANEL_RADIUS: f32 = 16.0;

/// `--dcc-surface-edge`.
pub(super) fn edge(cx: &App) -> Hsla {
    cx.theme().colors.border.opacity(0.8)
}

/// `color-mix(in srgb, var(--muted) 25%, var(--popover))`.
pub(super) fn quiet(cx: &App) -> Hsla {
    let colors = &cx.theme().colors;
    mix_oklab(colors.muted, 0.25, colors.popover)
}

pub(super) fn label(text: SharedString) -> impl IntoElement {
    div()
        .mb(px(7.0))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
}

/// A shadcn Textarea / Input box around one of the panel's fields.
pub(super) fn field(content: impl IntoElement, min_height: Option<f32>, cx: &App) -> gpui::Div {
    let colors = &cx.theme().colors;
    div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(edge(cx))
        .bg(colors.background)
        .px(px(12.0))
        .text_size(px(12.0))
        .text_color(colors.foreground)
        .cursor_text()
        .map(|field| match min_height {
            Some(height) => field.min_h(px(height)).py(px(8.0)).line_height(px(20.0)),
            None => field.h(px(32.0)).flex().items_center(),
        })
        .child(div().w_full().min_w_0().child(content))
}

fn panel(composer: &Composer, window: &mut Window, cx: &mut Context<Composer>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let calm = colors.accent_calm;
    let header = div()
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(12.0))
        .p(px(20.0))
        .rounded_t(crate::ui::inner_radius(px(PANEL_RADIUS), true))
        .bg(mix_oklab(calm, 0.07, colors.popover))
        .border_b_1()
        .border_color(edge(cx))
        .child(
            div()
                .flex()
                .size(px(40.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .rounded(px(12.0))
                .bg(calm.opacity(0.1))
                .child(Icon::new("target").size(21.0).color(calm)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(15.0))
                        .line_height(px(22.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(cx.t("composer.objective.title")),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(11.0))
                        .line_height(px(17.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("composer.objective.native.subtitle")),
                ),
        )
        .child(
            Button::new("objective-close")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .on_click(cx.listener(|composer, _, window, cx| {
                    composer.set_objective_open(false, window, cx)
                }))
                .child(Icon::new("x").size(15.0)),
        );
    // `max-height: min(760px, available)` less the header and the footer.
    let room = (f32::from(window.viewport_size().height) - 300.0).clamp(220.0, 560.0);
    let body = div()
        .id("objective-scroll")
        .flex()
        .flex_col()
        .min_h_0()
        .max_h(px(room))
        .overflow_y_scroll()
        .px(px(20.0))
        .py(px(18.0))
        .child(native::body(composer, cx));
    div()
        .debug_selector(|| "objective-panel".into())
        .flex()
        .flex_col()
        .rounded(px(PANEL_RADIUS))
        .overflow_hidden()
        .border_1()
        .border_color(edge(cx))
        .text_size(px(12.0))
        .line_height(px(18.0))
        .child(header)
        .child(body)
        .child(native::footer(composer, cx))
        .into_any_element()
}

/// The toolbar's "Objetivo" (`SessionObjectiveControl`).
pub(in crate::screens::composer) fn objective_control(
    composer: &Composer,
    window: &mut Window,
    cx: &mut Context<Composer>,
) -> Option<AnyElement> {
    composer.session.session_id.as_ref()?;
    if !composer.offers_goal() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    // The trigger follows the provider's goal: running, or waiting on you.
    let status = native::live_goal(composer).map(|goal| goal.status == GoalStatus::Active);
    let tone = match status {
        Some(true) => tw::EMERALD_600.hsla(),
        Some(false) => tw::AMBER_600.hsla(),
        None => colors.muted_foreground,
    };
    let dark = cx.theme().appearance == crate::theme::Appearance::Dark;
    let tone = match status {
        Some(true) if dark => tw::EMERALD_400.hsla(),
        Some(false) if dark => tw::AMBER_400.hsla(),
        _ => tone,
    };
    let hover = colors.accent.opacity(0.6);
    let trigger = div()
        .flex()
        .h(px(28.0))
        .flex_shrink_0()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .rounded(px(9.0))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .map(|trigger| match status {
            Some(_) => trigger
                .bg(colors.accent.opacity(0.6))
                .text_color(colors.foreground),
            None => trigger.text_color(colors.muted_foreground.opacity(0.7)),
        })
        .child(Icon::new("target").size(13.0).stroke(1.8).color(tone))
        .when(
            composer.fits(super::super::narrow::PLAN_LABELS),
            |trigger| {
                trigger.child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("composer.objective.compact")),
                )
            },
        );
    let title = cx.t("composer.objective.tooltipTitle");
    let description = cx.t("composer.objective.native.tooltip");
    let open = composer.objective.open;
    let trigger = Tooltip::new("objective-tip", trigger, move |_, cx| {
        div()
            .flex()
            .flex_col()
            .max_w(px(320.0))
            .child(div().font_weight(FontWeight::MEDIUM).child(title.clone()))
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(11.0))
                    .line_height(px(16.0))
                    .text_color(cx.theme().colors.background.opacity(0.75))
                    .child(description.clone()),
            )
            .into_any_element()
    })
    .enabled(!open);
    let card = open.then(|| panel(composer, window, cx));
    let entity = cx.entity().downgrade();
    Some(
        Popover::new(
            "objective",
            open,
            trigger,
            move |_, _| card.unwrap_or_else(|| div().into_any_element()),
            move |open, window, cx| {
                let _ = entity.update(cx, |composer, cx| {
                    composer.set_objective_open(open, window, cx)
                });
            },
        )
        .side(PopoverSide::Top)
        .side_offset(10.0)
        .width(PANEL_WIDTH)
        .padding(0.0)
        .into_any_element(),
    )
}
