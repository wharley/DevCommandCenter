//! The find bar a conversation shows under its header (`ThreadFindBar`):
//! the field (Enter for the next hit, Shift+Enter the one before, Esc to
//! close), how many hits there are with the excerpt of the current one,
//! ↑/↓ and ×. A task's conversation and the team room draw the same one;
//! each keeps its own field and hits.

use gpui::{AnyElement, Context, Entity, KeyDownEvent, Window, div, prelude::*, px};

use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::text_input::TextInput;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip, tabular_nums};

/// The bar's elements, as each screen's tests know them.
pub(crate) struct FindIds {
    pub bar: &'static str,
    pub field: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    pub close: &'static str,
    pub close_tip: &'static str,
}

/// What the bar shows now.
pub(crate) struct FindStrip<'a> {
    pub ids: &'static FindIds,
    pub input: &'a Entity<TextInput>,
    /// The current hit, counted from zero.
    pub active: usize,
    pub total: usize,
    pub snippet: Option<&'a str>,
}

pub(crate) fn find_strip<T: 'static>(
    strip: FindStrip<'_>,
    step: fn(&mut T, bool, &mut Context<T>),
    close: fn(&mut T, &mut Window, &mut Context<T>),
    cx: &mut Context<T>,
) -> AnyElement {
    let FindStrip {
        ids,
        input,
        active,
        total,
        snippet,
    } = strip;
    let colors = cx.theme().colors.clone();
    let radius = cx.theme().radius;
    let query = input.read(cx).text().trim().to_string();
    let status = if query.is_empty() {
        cx.t("conversation.find.hint").to_string()
    } else if total == 0 {
        cx.t("conversation.find.empty").to_string()
    } else {
        cx.t_with(
            "conversation.find.count",
            &Args::default()
                .with("current", (active + 1).to_string())
                .with("total", total.to_string()),
        )
        .to_string()
    };
    let snippet = snippet
        .map(|snippet| format!(" · {snippet}"))
        .unwrap_or_default();
    let nav = |id: &'static str, forward: bool, cx: &mut Context<T>| {
        let (icon, label) = match forward {
            true => ("chevron-down", "conversation.find.next"),
            false => ("chevron-up", "conversation.find.previous"),
        };
        Tooltip::text(
            id,
            Button::new(id)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .disabled(total == 0)
                .on_click(cx.listener(move |view, _, _, cx| step(view, forward, cx)))
                .child(Icon::new(icon).size(14.0)),
            cx.t(label),
        )
    };
    div()
        .debug_selector(|| ids.bar.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(8.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.95))
        .px(px(16.0))
        .py(px(6.0))
        .child(
            div()
                .id(ids.field)
                .flex()
                .h(px(28.0))
                .w_full()
                .max_w(px(320.0))
                .items_center()
                .rounded(radius.lg)
                .border_1()
                .border_color(colors.input)
                .px(px(10.0))
                .text_size(px(12.0))
                .on_key_down(cx.listener(move |view, event: &KeyDownEvent, _, cx| {
                    let keystroke = &event.keystroke;
                    if keystroke.key == "enter" && keystroke.modifiers.shift {
                        cx.stop_propagation();
                        step(view, false, cx);
                    }
                }))
                .child(input.clone()),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(px(11.0))
                .font_features(tabular_nums())
                .text_color(colors.muted_foreground)
                .child(format!("{status}{snippet}")),
        )
        .child(nav(ids.previous, false, cx))
        .child(nav(ids.next, true, cx))
        .child(Tooltip::text(
            ids.close_tip,
            Button::new(ids.close)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .on_click(cx.listener(move |view, _, window, cx| close(view, window, cx)))
                .child(Icon::new("x").size(14.0)),
            cx.t("conversation.find.close"),
        ))
        .into_any_element()
}
