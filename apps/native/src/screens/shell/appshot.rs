//! The Appshots key's capture (`app::appshot_key`) into the composer of the
//! task on screen; with no task open there is no composer to take it.

use gpui::Context;

use super::{Shell, Surface};
use crate::data::appshot_key::Captured;
use crate::i18n::Translate;
use crate::ui::Toaster;

impl Shell {
    pub(crate) fn take_appshot(&mut self, capture: Captured, cx: &mut Context<Self>) {
        let path = match capture {
            Ok(path) => path,
            Err(code) => {
                Toaster::error(cx.t(&crate::data::appshots::error_key(&code)), cx);
                return;
            }
        };
        if self.conversation.read(cx).workspace_path().is_none() {
            let _ = std::fs::remove_file(path);
            Toaster::error(cx.t("appshots.errors.unavailable"), cx);
            return;
        }
        self.show(Surface::Conversation, cx);
        self.conversation.update(cx, |conversation, cx| {
            conversation
                .composer
                .update(cx, |composer, cx| composer.add_paths(vec![path], cx));
            conversation.focus_composer_soon(cx);
        });
    }
}
