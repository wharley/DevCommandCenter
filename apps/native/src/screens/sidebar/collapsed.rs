//! The collapsed rail (76px): icon buttons on top, one initials tile per
//! task, the tools at the bottom.

use gpui::{ClickEvent, Context, SharedString, Window, div, prelude::*, px};

use super::footer::{TOOLS, tool_button};
use super::row::{initials, selected_background};
use super::{Sidebar, SidebarEvent};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

fn icon_button(
    id: &'static str,
    icon: &'static str,
    stroke: f32,
    event: SidebarEvent,
    cx: &mut Context<Sidebar>,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .child(Icon::new(icon).stroke(stroke))
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(event.clone())))
}

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> impl IntoElement {
    let has_tasks = !sidebar.state.is_empty();
    let has_selection = sidebar.state.selected_task_id.is_some();
    let mut tiles = Vec::new();
    for task in &sidebar.state.data.tasks {
        tiles.push(task_tile(
            sidebar,
            &task.id,
            &task.display_title(),
            &task.name,
            cx,
        ));
    }
    let mut tools = Vec::new();
    for (id, icon, event) in TOOLS {
        // The feedback button keeps its full size in the rail too.
        let size = if id == "tool-feedback" {
            ButtonSize::Icon
        } else {
            ButtonSize::IconXs
        };
        tools.push(tool_button(id, icon, event, size, cx));
    }
    let code_column = has_selection.then(|| {
        icon_button(
            "rail-code-column",
            "folder-tree",
            1.8,
            SidebarEvent::ToggleCodeColumn,
            cx,
        )
    });
    let project_buttons = has_tasks.then(|| {
        [
            icon_button(
                "rail-open-project",
                "folder-plus",
                1.9,
                SidebarEvent::OpenProject,
                cx,
            ),
            icon_button("rail-repo-picker", "plus", 2.2, SidebarEvent::NewTask, cx),
        ]
    });
    let expand = icon_button(
        "expand-sidebar",
        "panel-right",
        1.8,
        SidebarEvent::ToggleCollapsed,
        cx,
    );
    let new_task = icon_button("rail-new-task", "plus", 1.9, SidebarEvent::NewTask, cx);
    let pull_requests = icon_button(
        "rail-pull-requests",
        "git-pull-request",
        1.9,
        SidebarEvent::OpenPullRequests,
        cx,
    );
    let notes = icon_button(
        "rail-notes",
        "sticky-note",
        2.0,
        SidebarEvent::OpenNotes,
        cx,
    );
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.0))
        .py(px(8.0))
        .overflow_hidden()
        .child(expand)
        .children(code_column)
        .child(new_task)
        .child(pull_requests)
        .child(notes)
        .children(project_buttons.into_iter().flatten())
        .child(
            div()
                .id("rail-tasks")
                .flex_1()
                .min_h_0()
                .w_full()
                .px(px(4.0))
                .overflow_y_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.0))
                        .children(tiles),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0))
                .pb(px(8.0))
                .children(tools),
        )
}

fn task_tile(
    sidebar: &Sidebar,
    task_id: &str,
    title: &str,
    name: &str,
    cx: &mut Context<Sidebar>,
) -> gpui::AnyElement {
    let selected = sidebar.state.selected_task_id.as_deref() == Some(task_id);
    let theme = cx.theme();
    let colors = &theme.colors;
    let hover_background = colors.accent.opacity(0.6);
    let hover_text = colors.foreground;
    let id = task_id.to_string();
    div()
        .id(SharedString::from(format!("rail-task-{task_id}")))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(36.0))
        .rounded(theme.radius.lg)
        .border_1()
        .text_size(px(10.0))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(selected, |tile| {
            tile.bg(selected_background(cx))
                .text_color(colors.foreground)
                .border_color(colors.border)
        })
        .when(!selected, |tile| {
            tile.bg(colors.accent.opacity(0.35))
                .text_color(colors.muted_foreground)
                .border_color(gpui::transparent_black())
                .hover(move |style| style.bg(hover_background).text_color(hover_text))
        })
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| this.open_task(&id, cx)),
        )
        .child(SharedString::from(initials(if name.is_empty() {
            title
        } else {
            name
        })))
        .into_any_element()
}
