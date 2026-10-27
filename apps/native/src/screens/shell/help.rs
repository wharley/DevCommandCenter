//! The help and the shortcuts over the window (`openHelp(topic)` and
//! `setIsShortcutsOpen` in `App.tsx`): "?" in the sidebar and "Como
//! funciona" open the help; its footer swaps it for the shortcuts.

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::Shell;
use crate::screens::help::{Help, HelpEvent};
use crate::screens::shortcuts::ShortcutSheet;

pub(super) type OpenHelp = (Entity<Help>, Subscription);

impl Shell {
    /// `topic` is a help topic id ("workspaces"); none opens the default.
    pub(super) fn open_help(
        &mut self,
        topic: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let help = cx.new(|cx| Help::new(topic, window, cx));
        let events = cx.subscribe_in(&help, window, |shell, _, event, _, cx| {
            shell.help = None;
            if matches!(event, HelpEvent::OpenShortcuts) {
                shell.shortcuts_open = true;
            }
            cx.notify();
        });
        self.help = Some((help, events));
        cx.notify();
    }

    pub(super) fn help_overlays(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let help = self
            .help
            .as_ref()
            .map(|(help, _)| help.clone().into_any_element());
        let shell = cx.entity().downgrade();
        let shortcuts = self.shortcuts_open.then(|| {
            ShortcutSheet::new(move |_, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.shortcuts_open = false;
                    cx.notify();
                });
            })
            .into_any_element()
        });
        help.into_iter().chain(shortcuts).collect()
    }
}
