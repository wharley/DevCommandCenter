//! The delivery dialogs: "Revisar mensagem do commit" (subject and body,
//! both editable, in monospace; "Preparando…" while the conversation
//! menu's is read), the confirmation before a merge or a sync with the
//! base, and "Criar PR". Drawn by the `DeliveryOverlay`, so they open from
//! either header with the column shown or not.

use gpui::{
    AnyElement, Context, FocusHandle, Focusable, FontWeight, SharedString, Window, div, prelude::*,
    px,
};

use super::super::CodeColumn;
use super::{Confirm, PreviewOrigin};
use crate::i18n::{Args, Translate};
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer, dialog_header, spinner};

/// shadcn's `Input` (one line, 36px) or `Textarea` (`min-h-*`), in
/// monospace at 12px for a commit.
pub(super) fn field(
    child: AnyElement,
    focus: &FocusHandle,
    min_height: Option<f32>,
    mono: bool,
    window: &Window,
    cx: &Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let focused = focus.is_focused(window);
    let dark = cx.theme().appearance == Appearance::Dark;
    div()
        .when_some(min_height, |field, height| field.min_h(px(height)))
        .when(min_height.is_none(), |field| {
            field.h(px(36.0)).flex().items_center()
        })
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
        .px(px(if min_height.is_some() { 10.0 } else { 12.0 }))
        .when(min_height.is_some(), |field| field.py(px(8.0)))
        .when(mono, |field| {
            field.font_family(fonts::MONO).text_size(px(12.0))
        })
        .when(!mono, |field| field.text_size(px(14.0)))
        .child(child)
        .into_any_element()
}

pub(super) fn label(text: SharedString) -> AnyElement {
    div()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
        .into_any_element()
}

/// "Preparando a prévia do commit…" (`role="status"`).
fn preparing(cx: &Context<CodeColumn>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    div()
        .debug_selector(|| "commit-preview-preparing".into())
        .my(px(12.0))
        .flex()
        .min_h(px(208.0))
        .items_center()
        .justify_center()
        .gap(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.muted.opacity(0.2))
        .px(px(24.0))
        .py(px(32.0))
        .child(
            div()
                .flex_shrink_0()
                .text_color(colors.muted_foreground)
                .child(spinner("commit-preview-spinner", 20.0)),
        )
        .child(
            div()
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("commit.preview.preparing")),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .child(cx.t("commit.preview.preparingDescription")),
                ),
        )
        .into_any_element()
}

fn preview(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let preview = column.delivery.preview.as_ref();
    let preparing_push = column.delivery.preparing;
    if preview.is_none() && preparing_push.is_none() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let empty = preview.is_none_or(|preview| preview.subject.read(cx).text().trim().is_empty());
    let busy = column.delivery.busy;
    // Only a commit without its push says "Commitar".
    let commit_only = match preview.map(|preview| preview.origin) {
        Some(PreviewOrigin::Dock { push }) => !push,
        Some(PreviewOrigin::Review) => false,
        None => preparing_push == Some(false),
    };
    let header = dialog_header(
        cx.t("commit.preview.title"),
        Some(cx.t("commit.preview.description")),
        cx,
    );
    let fields = match preview {
        None => preparing(cx),
        Some(preview) => {
            let source = cx.t_with(
                "commit.preview.source",
                &Args::count(i64::from(preview.staged_files)),
            );
            let subject_focus = preview.subject.read(cx).focus_handle(cx);
            let body_focus = preview.body.read(cx).focus_handle(cx);
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .py(px(12.0))
                .child(label(cx.t("commit.preview.label")))
                .child(field(
                    preview.subject.clone().into_any_element(),
                    &subject_focus,
                    Some(96.0),
                    true,
                    window,
                    cx,
                ))
                .child(label(cx.t("commit.preview.bodyLabel")))
                .child(field(
                    preview.body.clone().into_any_element(),
                    &body_focus,
                    Some(80.0),
                    true,
                    window,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(source),
                )
                .into_any_element()
        }
    };
    let footer = dialog_footer(
        [
            Button::new("commit-preview-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .disabled(busy)
                .child(cx.t("commit.preview.cancel"))
                .on_click(cx.listener(|column, _, _, cx| column.close_preview(cx)))
                .into_any_element(),
            Button::new("commit-preview-confirm")
                .size(ButtonSize::Default)
                .disabled(empty || busy || preview.is_none())
                .child(if commit_only {
                    cx.t("composer.executionDock.actions.commit")
                } else {
                    cx.t("commit.preview.confirm")
                })
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
        Confirm::CompleteMerge => (
            cx.t("inspector.gitConfirmation.completeMergeTitle"),
            cx.t_with(
                "inspector.gitConfirmation.completeMergeDescription",
                &Args::count(column.conflicts_to_complete() as i64),
            ),
            cx.t("inspector.gitConfirmation.completeMergeConfirm"),
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

/// Whichever of the delivery dialogs is open.
pub(super) fn dialogs(
    column: &CodeColumn,
    window: &Window,
    cx: &mut Context<CodeColumn>,
) -> Vec<AnyElement> {
    preview(column, window, cx)
        .into_iter()
        .chain(confirm(column, cx))
        .chain(super::request::request_dialog(column, window, cx))
        .collect()
}
