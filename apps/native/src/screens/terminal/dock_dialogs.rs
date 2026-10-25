//! The dock's two dialogs: "Renomear terminal" (only the tab's name, the
//! shell goes on) and "Há um processo em execução" before closing a tab
//! whose program still runs.

use gpui::{AnyElement, Context, Focusable, Window, div, prelude::*, px};

use super::dock::TerminalDock;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{
    Button, ButtonSize, ButtonVariant, Dialog, dialog_footer, dialog_footer_split, dialog_header,
};

impl TerminalDock {
    pub(super) fn rename_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let renaming = self.renaming.as_ref()?;
        let colors = cx.theme().colors.clone();
        let radius = cx.theme().radius;
        let input = renaming.input.clone();
        let blank = input.read(cx).text().trim().is_empty();
        let input_focus = input.read(cx).focus_handle(cx);
        let focused = input_focus.is_focused(window);
        // shadcn's Input: `h-8 rounded-lg border-input px-2.5`, ring on focus.
        let field = div()
            .debug_selector(|| "terminal-rename-input".into())
            .flex()
            .items_center()
            .h(px(32.0))
            .px(px(10.0))
            .rounded(radius.lg)
            .border_1()
            .border_color(if focused { colors.ring } else { colors.input })
            .when(focused, |field| {
                field.shadow(vec![gpui::BoxShadow {
                    color: colors.ring.opacity(0.5),
                    offset: gpui::point(px(0.0), px(0.0)),
                    blur_radius: px(0.0),
                    spread_radius: px(3.0),
                    inset: false,
                }])
            })
            .text_size(px(14.0))
            .child(input);
        let dock = cx.entity().downgrade();
        let footer = dialog_footer(
            [
                Button::new("terminal-rename-cancel")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Default)
                    .on_click(cx.listener(|dock, _, _, cx| dock.cancel_rename(cx)))
                    .child(cx.t("terminalDock.cancelRename"))
                    .into_any_element(),
                Button::new("terminal-rename-save")
                    .size(ButtonSize::Default)
                    .disabled(blank)
                    .on_click(cx.listener(|dock, _, _, cx| dock.submit_rename(cx)))
                    .child(cx.t("terminalDock.saveRename"))
                    .into_any_element(),
            ],
            cx,
        );
        Some(
            Dialog::new("terminal-rename-dialog", move |_: &mut Window, cx| {
                let _ = dock.update(cx, |dock, cx| dock.cancel_rename(cx));
            })
            .initial_focus(input_focus)
            .child(dialog_header(
                cx.t("terminalDock.renameTitle"),
                Some(cx.t("terminalDock.renameDescription")),
                cx,
            ))
            .child(field)
            .child(footer)
            .into_any_element(),
        )
    }

    pub(super) fn close_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, process) = self.closing.as_ref()?;
        let dock = cx.entity().downgrade();
        let description = cx.t_with(
            "terminalDock.closeRunningDescription",
            &Args::default().with("process", process.clone()),
        );
        let footer = dialog_footer_split(
            Button::new("terminal-close-hide")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Default)
                .on_click(cx.listener(|dock, _, _, cx| dock.hide_instead(cx)))
                .child(cx.t("terminalDock.hideInstead"))
                .into_any_element(),
            [
                Button::new("terminal-close-cancel")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Default)
                    .on_click(cx.listener(|dock, _, _, cx| dock.cancel_close(cx)))
                    .child(cx.t("terminalDock.cancelClose"))
                    .into_any_element(),
                Button::new("terminal-close-confirm")
                    .variant(ButtonVariant::Destructive)
                    .size(ButtonSize::Default)
                    .on_click(cx.listener(|dock, _, _, cx| dock.confirm_close(cx)))
                    .child(cx.t("terminalDock.terminateAndClose"))
                    .into_any_element(),
            ],
            cx,
        );
        Some(
            Dialog::new("terminal-close-dialog", move |_: &mut Window, cx| {
                let _ = dock.update(cx, |dock, cx| dock.cancel_close(cx));
            })
            .max_width(448.0)
            .child(dialog_header(
                cx.t("terminalDock.closeRunningTitle"),
                Some(description),
                cx,
            ))
            .child(footer)
            .into_any_element(),
        )
    }
}
