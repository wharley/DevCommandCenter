//! The header's dialogs: "Revisar mensagem do commit" (subject and body,
//! both editable, in monospace) and the confirmation before a merge or a
//! sync with the base.

use gpui::{
    AnyElement, Context, Entity, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};

use super::super::CodeColumn;
use super::Confirm;
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::text_area::TextArea;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer, dialog_header};

/// shadcn's `Textarea` with `min-h-* resize-y font-mono text-[12px]`.
fn field(
    area: &Entity<TextArea>,
    min_height: f32,
    window: &Window,
    cx: &Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let focused = area.read(cx).focus_handle(cx).is_focused(window);
    let dark = cx.theme().appearance == Appearance::Dark;
    div()
        .min_h(px(min_height))
        .w_full()
        .rounded(px(8.0))
        .border_1()
        .border_color(if focused { colors.ring } else { colors.input })
        .when(dark, |field| field.bg(colors.input.opacity(0.3)))
        .when(focused, |field| {
            field.shadow(vec![gpui::BoxShadow {
                color: colors.ring.opacity(0.5),
                offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: px(3.0),
                inset: false,
            }])
        })
        .px(px(10.0))
        .py(px(8.0))
        .font_family(fonts::MONO)
        .text_size(px(12.0))
        .child(area.clone())
        .into_any_element()
}

fn label(text: SharedString) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
        .into_any_element()
}

fn preview(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let preview = column.delivery.preview.as_ref()?;
    let colors = cx.theme().colors.clone();
    let empty = preview.subject.read(cx).text().trim().is_empty();
    let busy = column.delivery.busy;
    let header = dialog_header(
        cx.t("commit.preview.title"),
        Some(cx.t("commit.preview.description")),
        cx,
    );
    let source = cx.t_with(
        "commit.preview.source",
        &Args::count(i64::from(preview.staged_files)),
    );
    let fields = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(12.0))
        .child(label(cx.t("commit.preview.label")))
        .child(field(&preview.subject, 96.0, window, cx))
        .child(label(cx.t("commit.preview.bodyLabel")))
        .child(field(&preview.body, 80.0, window, cx))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(colors.muted_foreground)
                .child(source),
        );
    let footer = dialog_footer(
        [
            Button::new("commit-preview-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("commit.preview.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.close_preview(cx)))
                .into_any_element(),
            Button::new("commit-preview-confirm")
                .size(ButtonSize::Default)
                .disabled(empty || busy)
                .child(cx.t("commit.preview.confirm"))
                .on_click(cx.listener(|column, _, _, cx| column.confirm_preview(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("commit-preview-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.close_preview(cx));
        })
        .max_width(512.0)
        .gap(0.0)
        .child(header)
        .child(fields)
        .child(footer)
        .into_any_element(),
    )
}

fn confirm(column: &CodeColumn, cx: &mut Context<CodeColumn>) -> Option<AnyElement> {
    let confirm = column.delivery.confirm?;
    let label = column.delivery.request_label();
    let args = Args::default().with("requestLabel", label);
    let (title, description, action) = match confirm {
        Confirm::Merge => (
            cx.t_with("inspector.gitConfirmation.mergeTitle", &args),
            cx.t_with("inspector.gitConfirmation.mergeDescription", &args),
            cx.t_with("inspector.gitConfirmation.mergeConfirm", &args),
        ),
        Confirm::SyncBase => (
            cx.t("inspector.gitConfirmation.syncTitle"),
            match column
                .delivery
                .pr
                .as_ref()
                .and_then(|pr| pr.base_branch.clone())
            {
                Some(base) => cx.t_with(
                    "inspector.gitConfirmation.syncDescriptionWithBase",
                    &Args::default().with("baseBranch", base),
                ),
                None => cx.t("inspector.gitConfirmation.syncDescription"),
            },
            cx.t("inspector.gitConfirmation.syncConfirm"),
        ),
    };
    let colors = cx.theme().colors.clone();
    // `DialogDescription className="text-[12px] leading-relaxed"`
    let header = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(description),
        );
    let busy = column.delivery.busy || column.delivery.syncing;
    let footer = dialog_footer(
        [
            Button::new("delivery-confirm-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .child(cx.t("inspector.gitConfirmation.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.cancel_confirm(cx)))
                .into_any_element(),
            Button::new("delivery-confirm-run")
                .variant(if confirm == Confirm::Merge {
                    ButtonVariant::Destructive
                } else {
                    ButtonVariant::Default
                })
                .size(ButtonSize::Default)
                .disabled(busy)
                .child(action)
                .on_click(cx.listener(|column, _, _, cx| column.run_confirmed(cx)))
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("delivery-confirm-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |column, cx| column.cancel_confirm(cx));
        })
        .close_button(false)
        .child(header)
        .child(footer)
        .into_any_element(),
    )
}

/// Whichever of the header's dialogs is open.
pub(crate) fn dialogs(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Vec<AnyElement> {
    preview(column, window, cx)
        .into_iter()
        .chain(confirm(column, cx))
        .collect()
}
