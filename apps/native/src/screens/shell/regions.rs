//! The shell's regions. Each screen takes one over as it is migrated:
//! the sidebar is screen 1, the main area hosts screens 2–4 and 6–8.

use gpui::{App, Window, div, prelude::*, px};

use crate::theme::ActiveTheme;

/// Left column (`<aside data-dcc-sidebar-root>`), `bg-sidebar`.
#[derive(IntoElement)]
pub struct SidebarRegion {
    width: f32,
}

impl SidebarRegion {
    pub fn new(width: f32) -> Self {
        Self { width }
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
