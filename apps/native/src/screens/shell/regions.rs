//! The shell's regions. Each screen takes one over as it is migrated:
//! the sidebar is screen 1, the main area hosts screens 2–4 and 6–8.

use gpui::{AnyElement, App, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;

/// Left column (`<aside data-dcc-sidebar-root>`), `bg-sidebar`.
#[derive(IntoElement)]
pub struct SidebarRegion {
    width: f32,
    content: Option<AnyElement>,
}

impl SidebarRegion {
    pub fn new(width: f32) -> Self {
        Self {
            width,
            content: None,
        }
    }

    pub fn child(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_any_element());
        self
    }
}

impl RenderOnce for SidebarRegion {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .w(px(self.width))
            .overflow_hidden()
            .bg(cx.theme().colors.sidebar)
            .children(self.content)
    }
}

/// Main workspace panel (`<section>`), `bg-background`.
#[derive(IntoElement, Default)]
pub struct MainRegion;

impl RenderOnce for MainRegion {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .overflow_hidden()
            .bg(cx.theme().colors.background)
    }
}
