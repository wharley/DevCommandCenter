//! The window's frame: sidebar, resize separator and main area
//! (`App.tsx` root `<main>`). Owns the layout and follows the OS theme.

pub mod layout;
mod regions;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui::{
    Context, DragMoveEvent, Entity, MouseUpEvent, Pixels, Subscription, Window, div, prelude::*, px,
};

use crate::prefs::{self, Prefs};
use crate::screens::conversation::ConversationView;
use crate::screens::sidebar::{Sidebar, SidebarEvent};
use crate::theme::{ActiveTheme, Theme};
use crate::ui::{ResizeHandle, Toaster};
use layout::ShellLayout;
use regions::{MainRegion, SidebarRegion};

/// Drag payload of the sidebar separator.
#[derive(Clone, Copy)]
struct SidebarResize;

pub struct Shell {
    layout: ShellLayout,
    prefs: Prefs,
    prefs_path: PathBuf,
    sidebar: Entity<Sidebar>,
    conversation: Entity<ConversationView>,
    _appearance: Subscription,
    _sidebar_events: Subscription,
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
        let sidebar = cx.new(|cx| {
            Sidebar::new(
                prefs.sidebar_collapsed,
                prefs.waiting_open,
                prefs.completed_open,
                prefs::native_file("seen-results.json"),
                cx,
            )
        });
        let sidebar_events = cx.subscribe(&sidebar, Self::on_sidebar_event);
        let conversation = cx.new(|cx| ConversationView::new(window, cx));
        Self {
            layout: ShellLayout::new(prefs.sidebar_width, prefs.sidebar_collapsed),
            prefs,
            prefs_path,
            sidebar,
            conversation,
            _appearance: appearance,
            _sidebar_events: sidebar_events,
        }
    }

    /// Screens the sidebar opens arrive one by one; until then their events
    /// only matter here when they change the frame.
    fn on_sidebar_event(
        &mut self,
        sidebar: Entity<Sidebar>,
        event: &SidebarEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SidebarEvent::ToggleCollapsed => {
                self.layout.toggle_sidebar();
                let collapsed = self.layout.sidebar_collapsed;
                sidebar.update(cx, |sidebar, cx| sidebar.set_collapsed(collapsed, cx));
                self.save_layout(cx);
                cx.notify();
            }
            SidebarEvent::OpenTask(task_id) => {
                let task_id = task_id.clone();
                self.conversation
                    .update(cx, |conversation, cx| conversation.open(&task_id, None, cx));
                cx.notify();
            }
            SidebarEvent::OpenSession {
                task_id,
                session_id,
            } => {
                let (task_id, session_id) = (task_id.clone(), session_id.clone());
                self.conversation.update(cx, |conversation, cx| {
                    conversation.open(&task_id, Some(session_id), cx)
                });
                cx.notify();
            }
            _ => self.save_layout(cx),
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
            self.save_layout(cx);
            cx.notify();
        }
    }

    fn save_layout(&mut self, cx: &mut Context<Self>) {
        let (waiting_open, completed_open) = self.sidebar.read(cx).section_state();
        let next = Prefs {
            sidebar_width: self.layout.sidebar_width,
            sidebar_collapsed: self.layout.sidebar_collapsed,
            waiting_open,
            completed_open,
            ..self.prefs.clone()
        };
        if next == self.prefs {
            return;
        }
        self.prefs = next;
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
            // The desktop's base text: 16px with Tailwind preflight's 1.5 line height.
            .text_size(px(16.0))
            .line_height(gpui::relative(1.5))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SidebarResize>, _, cx| {
                    this.resize_sidebar(event.event.position.x, cx)
                }),
            )
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _, cx| {
                this.finish_sidebar_resize(event.position.x, cx)
            }))
            .child(SidebarRegion::new(self.layout.sidebar_rail_width()).child(self.sidebar.clone()))
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
            .child(MainRegion::default().child(self.conversation.clone()))
            .children(Toaster::handle(cx))
    }
}
