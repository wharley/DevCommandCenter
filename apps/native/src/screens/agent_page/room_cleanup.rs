//! The cleanup notice in the team room (`data::cleanup_alert`): past the
//! limit the person chose in Settings, "Suas concluídas ocupam 12 GB" with
//! "Liberar espaço" (the sidebar's dialog, which deletes only what is safe
//! and only on a click) and "Dispensar" (quiet until a gigabyte more).

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::room::TeamRoom;
use crate::data::cleanup_alert::CleanupAlert;
use crate::data::disk::format_disk_bytes;
use crate::i18n::{Args, I18n, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster};

impl TeamRoom {
    /// The limit and the dismissal, read again when the room opens.
    pub(crate) fn reload_cleanup(&mut self) {
        if let Some(path) = &self.cleanup_path {
            self.cleanup = CleanupAlert::load(path);
        }
    }

    fn dismiss_cleanup(&mut self, total: u64, cx: &mut Context<Self>) {
        match &self.cleanup_path {
            Some(path) => match CleanupAlert::dismiss(path, total) {
                Ok(alert) => self.cleanup = alert,
                Err(error) => Toaster::error(error.to_string(), cx),
            },
            None => self.cleanup.dismissed_at_bytes = Some(total),
        }
        cx.notify();
    }

    pub(super) fn cleanup_card(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let total = self.sidebar.read(cx).completed_bytes()?;
        if !self.cleanup.shows(total) {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let size = format_disk_bytes(total, cx.global::<I18n>().locale());
        let sidebar = self.sidebar.clone();
        Some(
            div()
                .debug_selector(|| "room-cleanup".into())
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.card)
                .child(
                    Icon::new("brush-cleaning")
                        .size(18.0)
                        .color(colors.muted_foreground),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.5))
                        .text_color(colors.foreground)
                        .child(cx.t_with(
                            "agents.room.cleanup.text",
                            &Args::default().with("total", size),
                        )),
                )
                .child(
                    Button::new("room-cleanup-open")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Xs)
                        .on_click(move |_, _, cx| {
                            sidebar.update(cx, |sidebar, cx| sidebar.open_cleanup(cx))
                        })
                        .child(cx.t("agents.room.cleanup.open")),
                )
                .child(
                    Button::new("room-cleanup-dismiss")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Xs)
                        .on_click(
                            cx.listener(move |room, _, _, cx| room.dismiss_cleanup(total, cx)),
                        )
                        .child(cx.t("agents.room.cleanup.dismiss")),
                )
                .into_any_element(),
        )
    }
}
