//! The resolver's questions (`window.confirm` on the desktop): the text and
//! Cancelar / OK.

use gpui::{AnyElement, Context, Window, div, prelude::*, px};

use super::{MergeResolver, Pending};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Dialog, dialog_footer};

pub(crate) fn confirm_dialog(
    resolver: &MergeResolver,
    cx: &mut Context<MergeResolver>,
) -> Option<AnyElement> {
    let pending = resolver.pending.clone()?;
    let text = match &pending {
        Pending::DiscardForFile(_) | Pending::DiscardToClose => {
            cx.t("mergeConflict.confirm.discardEdit")
        }
        Pending::RemainingMarkers => cx.t("mergeConflict.confirm.remainingMarkers"),
        Pending::EmptyResult => cx.t("mergeConflict.confirm.emptyResult"),
        Pending::DeleteResult => cx.t_with(
            "mergeConflict.confirm.deleteResult",
            &Args::default().with(
                "path",
                resolver
                    .open
                    .as_ref()
                    .map(|open| open.entry.path.clone())
                    .unwrap_or_default(),
            ),
        ),
        Pending::Abort => cx.t("mergeConflict.confirm.abort"),
        Pending::OpenAgentSession(_) => cx.t("mergeConflict.confirm.openAgentSession"),
        Pending::ValidationCommands => {
            let commands = match &resolver.validation {
                Some(super::Load::Ready(config)) => config
                    .commands
                    .iter()
                    .map(|command| format!("• {command}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            };
            cx.t_with(
                "mergeConflict.confirm.validationCommands",
                &Args::default().with("commands", commands),
            )
        }
    };
    let colors = cx.theme().colors.clone();
    let footer = dialog_footer(
        [
            Button::new("merge-confirm-cancel")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.cancel_pending(cx)))
                .child(cx.t("inspector.gitConfirmation.cancel"))
                .into_any_element(),
            Button::new("merge-confirm-ok")
                .variant(ButtonVariant::Default)
                .size(ButtonSize::Default)
                .on_click(cx.listener(|resolver, _, _, cx| resolver.confirm(cx)))
                .child("OK")
                .into_any_element(),
        ],
        cx,
    );
    let close = cx.entity().downgrade();
    Some(
        Dialog::new("merge-confirm-dialog", move |_: &mut Window, cx| {
            let _ = close.update(cx, |resolver, cx| resolver.cancel_pending(cx));
        })
        .close_button(false)
        .max_width(448.0)
        .child(
            div()
                .debug_selector(|| "merge-confirm-text".into())
                .text_size(px(13.0))
                .line_height(gpui::relative(1.6))
                .text_color(colors.foreground)
                .children(
                    text.split('\n')
                        .map(|line| div().min_h(px(13.0 * 1.6)).child(line.to_string()))
                        .collect::<Vec<_>>(),
                ),
        )
        .child(footer)
        .into_any_element(),
    )
}
