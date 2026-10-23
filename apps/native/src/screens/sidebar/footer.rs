//! The tools row at the bottom (`.dcc-sidebar-tools`): usage, skills,
//! feedback, settings and help.

use gpui::{Context, ElementId, div, prelude::*, px};

use super::navigation::surface_edge;
use super::{Sidebar, SidebarEvent};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

/// Footer tools, in the desktop's order, with the icon each one uses.
pub const TOOLS: [(&str, &str, SidebarEvent); 5] = [
    ("tool-usage", "chart-column", SidebarEvent::OpenUsage),
    ("tool-skills", "sparkles", SidebarEvent::OpenSkills),
    (
        "tool-feedback",
        "message-square-more",
        SidebarEvent::OpenFeedback,
    ),
    ("tool-settings", "settings-2", SidebarEvent::OpenSettings),
    ("tool-help", "circle-question-mark", SidebarEvent::OpenHelp),
];

pub fn tool_button(
    id: &'static str,
    icon: &'static str,
    event: SidebarEvent,
    size: ButtonSize,
    cx: &mut Context<Sidebar>,
) -> Button {
    Button::new(ElementId::from(id))
        .variant(ButtonVariant::Ghost)
        .size(size)
        .muted()
        .child(Icon::new(icon).stroke(1.85))
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(event.clone())))
}

pub fn render(_sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let mut tools = Vec::new();
    for (id, icon, event) in TOOLS {
        tools.push(tool_button(id, icon, event, ButtonSize::Icon, cx));
    }
    let edge = surface_edge(cx.theme());
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_start()
        .gap(px(4.0))
        .mx(px(10.0))
        .px(px(4.0))
        .pt(px(8.0))
        .pb(px(12.0))
        .border_t_1()
        .border_color(edge)
        .children(tools)
}
