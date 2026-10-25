//! The stage's questions: the file changed on disk under an edit
//! (`ReconcileDialog`: the disk beside the edit, overwrite, take the disk,
//! cancel) and "Descartar alterações não salvas?" before an unsaved tab
//! goes (the desktop's `window.confirm`).

use gpui::{AnyElement, Context, FontWeight, Window, div, prelude::*, px};

use super::Stage;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer, dialog_header};

fn reconcile(stage: &Stage, window: &Window, cx: &mut Context<Stage>) -> Option<AnyElement> {
    let reconcile = stage.reconcile.as_ref()?;
    let colors = cx.theme().colors.clone();
    let saving = stage.saving_reconcile(cx);
    let viewport = window.viewport_size();
    // `max-h-[min(80vh,720px)] w-[min(92vw,920px)]`
    let height = (f32::from(viewport.height) * 0.8).min(720.0);
    let width = (f32::from(viewport.width) * 0.92).min(920.0);
    let header = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(cx.t("fileSurface.reconcile.title")),
        )
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground)
                .child(cx.t_with(
                    "fileSurface.reconcile.description",
                    &Args::default().with("name", reconcile.name.clone()),
                )),
        );
    let label = |key: &str, cx: &Context<Stage>| div().flex_1().child(cx.t(key).to_uppercase());
    let labels = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(6.0))
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .child(label("fileSurface.reconcile.diskLabel", cx))
        .child(label("fileSurface.reconcile.mineLabel", cx));
    let footer = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div().mr_auto().child(
                Button::new("reconcile-cancel")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .disabled(saving)
                    .on_click(cx.listener(|stage, _, _, cx| stage.cancel_reconcile(cx)))
                    .child(cx.t("fileSurface.reconcile.cancel")),
            ),
        )
        .child(
            Button::new("reconcile-take-disk")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .disabled(saving)
                .on_click(cx.listener(|stage, _, _, cx| stage.take_disk(cx)))
                .child(cx.t("fileSurface.reconcile.takeDisk")),
        )
        .child(
            Button::new("reconcile-overwrite")
                .size(ButtonSize::Sm)
                .disabled(saving)
                .on_click(cx.listener(|stage, _, _, cx| stage.overwrite(cx)))
                .child(cx.t("fileSurface.reconcile.overwrite")),
        );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("reconcile-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |stage, cx| stage.cancel_reconcile(cx));
        })
        .max_width(width)
        .max_height(height)
        .padding(0.0)
        .gap(0.0)
        .close_button(false)
        .child(
            div()
                .debug_selector(|| "reconcile-dialog".into())
                .flex()
                .flex_col()
                .w(px(width))
                .h(px(height))
                .child(header)
                .child(labels)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .border_y_1()
                        .border_color(colors.border.opacity(0.6))
                        .child(reconcile.diff.clone()),
                )
                .child(footer),
        )
        .into_any_element(),
    )
}

fn discard(stage: &Stage, cx: &mut Context<Stage>) -> Option<AnyElement> {
    stage.discard.as_ref()?;
    let header = dialog_header(cx.t("fileSurface.discardConfirm"), None, cx);
    let footer = dialog_footer(
        [
            Button::new("discard-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .on_click(cx.listener(|stage, _, _, cx| stage.cancel_discard(cx)))
                .child(cx.t("fileSurface.reconcile.cancel"))
                .into_any_element(),
            div()
                .debug_selector(|| "discard-confirm".into())
                .child(
                    Button::new("discard-confirm")
                        .size(ButtonSize::Default)
                        .on_click(
                            cx.listener(|stage, _, window, cx| stage.confirm_discard(window, cx)),
                        )
                        .child("OK"),
                )
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("discard-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |stage, cx| stage.cancel_discard(cx));
        })
        .child(header)
        .child(footer)
        .into_any_element(),
    )
}

pub(crate) fn dialogs(stage: &Stage, window: &Window, cx: &mut Context<Stage>) -> Vec<AnyElement> {
    reconcile(stage, window, cx)
        .into_iter()
        .chain(discard(stage, cx))
        .collect()
}
