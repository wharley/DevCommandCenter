//! The window's frame: sidebar, resize separator and main area
//! (`App.tsx` root `<main>`). Owns the layout and follows the OS theme.

pub mod layout;
mod regions;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui::{
    Context, DragMoveEvent, MouseUpEvent, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::prefs::Prefs;
use crate::theme::{ActiveTheme, Theme};
use crate::ui::ResizeHandle;
use layout::ShellLayout;
use regions::{MainRegion, SidebarRegion};

/// Drag payload of the sidebar separator.
#[derive(Clone, Copy)]
struct SidebarResize;

pub struct Shell {
    layout: ShellLayout,
    prefs: Prefs,
    prefs_path: PathBuf,
    _appearance: Subscription,
}

impl Shell {
    pub fn new(
        prefs: Prefs,
        prefs_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, window, cx| {
            if Theme::sync_with_window(window, cx) {
                cx.notify();
            }
        });
        Self {
            layout: ShellLayout::new(prefs.sidebar_width, prefs.sidebar_collapsed),
            prefs,
            prefs_path,
            _appearance: appearance,
        }
    }

    fn start_sidebar_resize(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        self.layout.start_resize(f32::from(pointer_x));
        cx.notify();
    }

    fn resize_sidebar(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        if self.layout.resize_to(f32::from(pointer_x)) {
            cx.notify();
        }
    }

    /// Applies the release position too: GPUI swallows the move that starts a
    /// drag, so a quick drag-and-release would otherwise end short.
    fn finish_sidebar_resize(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        self.layout.resize_to(f32::from(pointer_x));
        if self.layout.finish_resize() {
            self.save_layout();
            cx.notify();
        }
    }

    fn save_layout(&mut self) {
        self.prefs.sidebar_width = self.layout.sidebar_width;
        self.prefs.sidebar_collapsed = self.layout.sidebar_collapsed;
        if let Err(error) = self.prefs.save(&self.prefs_path) {
            eprintln!("[dcc-native] could not save the layout: {error}");
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let shell = cx.entity().downgrade();

        div()
            .id("shell")
            .relative()
            .flex()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .font_family(theme.fonts.sans.clone())
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SidebarResize>, _, cx| {
                    this.resize_sidebar(event.event.position.x, cx)
                }),
            )
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _, cx| {
                this.finish_sidebar_resize(event.position.x, cx)
            }))
            .child(SidebarRegion::new(self.layout.sidebar_rail_width()))
            .child(
                ResizeHandle::new("sidebar-resize", SidebarResize)
                    .left(px(self.layout.separator_left()))
                    .active(self.layout.is_resizing())
                    .on_start(move |pointer_x, _, cx| {
                        shell
                            .update(cx, |this, cx| this.start_sidebar_resize(pointer_x, cx))
                            .ok();
                    }),
            )
            .child(MainRegion)
    }
}
