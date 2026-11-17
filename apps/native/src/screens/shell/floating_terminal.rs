//! The floating terminal over every screen (`terminal/floating.rs`): made
//! with the shell, toggled by ⌘⌥T, the sidebar's button and the palette,
//! and placed over the main area, right of the sidebar. It shares nothing
//! with the task's dock but the desktop's activity, read once by that dock.

use gpui::{AnyElement, AppContext, Context, Entity, Subscription, Window, div, prelude::*, px};

use super::{Shell, Surface};
use crate::data::desktop_api::DesktopApi;
use crate::screens::terminal::tabs::TabStore;
use crate::screens::terminal::{DockEvent, TerminalDock};

/// Its gap to the window's edges and the sidebar.
const MARGIN: f32 = 12.0;

pub(super) fn new_floating(
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Entity<TerminalDock>, Subscription) {
    // Tests reach neither the running app's shells nor its files.
    let (tabs, api) = if cfg!(test) {
        (TabStore::default(), None)
    } else {
        (
            TabStore::load(&TerminalDock::floating_tabs_path()),
            DesktopApi::load(),
        )
    };
    let dock = cx.new(|_| TerminalDock::floating(tabs, api));
    let events = cx.subscribe_in(&dock, window, Shell::on_floating_event);
    (dock, events)
}

impl Shell {
    pub(super) fn toggle_floating_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.floating_terminal
            .update(cx, |dock, cx| dock.toggle_floating(window, cx));
    }

    fn on_floating_event(
        &mut self,
        _: &Entity<TerminalDock>,
        event: &DockEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            // The focus goes back where typing goes on the screen below.
            DockEvent::Hidden => {
                if self.surface == Surface::Conversation
                    && self.sidebar.read(cx).has_open_task() == Some(true)
                {
                    self.conversation
                        .update(cx, |conversation, cx| conversation.focus_composer_soon(cx));
                } else {
                    window.focus(&self.focus, cx);
                }
                cx.notify();
            }
            DockEvent::Expanded(_) | DockEvent::Resized(_) => cx.notify(),
            // Its bar offers neither.
            DockEvent::SendToAgent(_) | DockEvent::OpenServer(_) => {}
        }
    }

    /// The panel over the main area: its height at the bottom, or all of it
    /// when expanded.
    pub(super) fn floating_terminal_overlay(&self, cx: &gpui::App) -> Option<AnyElement> {
        let dock = self.floating_terminal.read(cx);
        if !dock.open {
            return None;
        }
        let left = self.layout.sidebar_rail_width() + MARGIN;
        let height = dock.height;
        let expanded = dock.expanded;
        Some(
            div()
                .absolute()
                .left(px(left))
                .right(px(MARGIN))
                .bottom(px(MARGIN))
                .map(|panel| {
                    if expanded {
                        panel.top(px(MARGIN))
                    } else {
                        panel.h(px(height))
                    }
                })
                .child(self.floating_terminal.clone())
                .into_any_element(),
        )
    }
}
