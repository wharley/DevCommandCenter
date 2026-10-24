//! The floating bar over the workspace's changes ("Reverter tudo" and
//! "Preparar tudo", or "Remover tudo" once nothing is left unstaged) and the
//! confirmation before reverting everything.

use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, Hsla, SharedString, Window, div, point, prelude::*,
    px,
};

use super::super::CodeColumn;
use super::Scope;
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, Icon, dialog_footer, dialog_header};

/// `Button variant="ghost" size="sm"` with `h-8 rounded-full gap-1.5 px-3`.
fn pill_button(
    id: &'static str,
    icon: &'static str,
    label: SharedString,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    cx: &Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = if cx.theme().appearance == Appearance::Dark {
        colors.accent.opacity(0.5)
    } else {
        colors.accent
    };
    let hover_text = colors.accent_foreground;
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .h(px(32.0))
        .items_center()
        .gap(px(6.0))
        .rounded_full()
        .px(px(12.0))
        .text_size(px(14.0))
        .font_weight(gpui::FontWeight::MEDIUM)
        .map(|button| {
            if disabled {
                button.opacity(0.5)
            } else {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover).text_color(hover_text))
                    .on_click(on_click)
            }
        })
        .child(Icon::new(icon).size(14.0))
        .child(label)
        .into_any_element()
}

pub(super) fn pill(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let busy = column.changes.busy;
    let conflicts = column
        .status
        .as_ref()
        .is_some_and(|status| status.conflict_count > 0);
    let unstaged = column
        .status
        .as_ref()
        .is_some_and(|status| !status.unstaged.is_empty());
    let revert = pill_button(
        "changes-revert-all",
        "undo-2",
        cx.t("inspector.changes.revertAll"),
        busy || conflicts,
        cx.listener(|column, _, _, cx| column.ask_revert_all(true, cx)),
        cx,
    );
    let second = if unstaged {
        pill_button(
            "changes-stage-all",
            "plus",
            cx.t("inspector.changes.stageAll"),
            busy || conflicts,
            cx.listener(|column, _, _, cx| column.stage_all(cx)),
            cx,
        )
    } else {
        pill_button(
            "changes-unstage-all",
            "minus",
            cx.t("inspector.changes.unstageAll"),
            busy,
            cx.listener(|column, _, _, cx| column.unstage_all(cx)),
            cx,
        )
    };
    // `shadow-lg`
    let shadow = |y: f32, blur: f32, spread: f32| BoxShadow {
        color: Hsla::black().opacity(0.1),
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(12.0))
        .flex()
        .justify_center()
        .px(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.background.opacity(0.95))
                .p(px(4.0))
                .shadow(vec![shadow(10.0, 15.0, -3.0), shadow(4.0, 6.0, -4.0)])
                .child(revert)
                .child(
                    div()
                        .mx(px(2.0))
                        .h(px(20.0))
                        .w(px(1.0))
                        .bg(colors.border.opacity(0.7)),
                )
                .child(second),
        )
        .into_any_element()
}

/// "Reverter todas as mudanças?" with the count of files (`sm:max-w-md`).
pub(super) fn revert_dialog(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> AnyElement {
    let count = column.changes.entries(Scope::Working).count() as i64;
    let header = dialog_header(
        cx.t("inspector.changes.revertAllConfirmTitle"),
        Some(cx.t_with(
            "inspector.changes.revertAllConfirmDescription",
            &Args::count(count),
        )),
        cx,
    );
    let footer = dialog_footer(
        [
            Button::new("changes-revert-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("inspector.changes.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.ask_revert_all(false, cx)))
                .into_any_element(),
            Button::new("changes-revert-confirm")
                .variant(ButtonVariant::Destructive)
                .size(ButtonSize::Default)
                .disabled(column.changes.busy)
                .child(cx.t("inspector.changes.revertAllConfirmAction"))
                .on_click(cx.listener(|column, _, _, cx| column.revert_all(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Dialog::new("changes-revert-dialog", move |_: &mut Window, cx| {
        let _ = close.update(cx, |column, cx| column.ask_revert_all(false, cx));
    })
    .max_width(448.0)
    .child(header)
    .child(footer)
    .into_any_element()
}
