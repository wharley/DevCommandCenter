//! "Geral" › "Avisos das salas": whether an agent asking, done or stopped in
//! a room is told while the person looks elsewhere (`shell::room_notices`).

use gpui::{AnyElement, Context, div, prelude::*, px};

use super::Settings;
use super::general::{card, heading};
use crate::data::room_notices_pref::RoomNoticesPref;
use crate::i18n::Translate;
use crate::ui::{Switch, Toaster};

impl Settings {
    pub(super) fn room_notices_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        card(cx)
            .debug_selector(|| "settings-room-notices".into())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(24.0))
                    .child(heading(
                        cx.t("settings.general.roomNotices.title"),
                        cx.t("settings.general.roomNotices.body"),
                        cx,
                    ))
                    .child(
                        Switch::new("settings-room-notices-switch", RoomNoticesPref::enabled(cx))
                            .on_toggle(move |on, _, cx| {
                                if let Err(error) = RoomNoticesPref::set(on, cx) {
                                    Toaster::error(error.to_string(), cx);
                                }
                                let _ = entity.update(cx, |_, cx| cx.notify());
                            }),
                    ),
            )
            .into_any_element()
    }
}
