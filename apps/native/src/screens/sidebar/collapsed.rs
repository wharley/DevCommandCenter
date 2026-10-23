//! The collapsed rail (76px): icon buttons on top, one initials tile per
//! task, the tools at the bottom.

use gpui::{ClickEvent, Context, SharedString, Window, div, prelude::*, px};

use super::footer::{TOOLS, tool_button};
use super::header::code_column_toggle;
use super::row::{initials, selected_background};
use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::tooltip::Side;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tooltip};

/// A rail button with its label beside it (`TooltipContent side="right"`).
fn icon_button(
    id: &'static str,
    icon: &'static str,
    stroke: f32,
    event: SidebarEvent,
    label: &str,
    cx: &mut Context<Sidebar>,
) -> gpui::AnyElement {
    let button = Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .muted()
        .child(Icon::new(icon).stroke(stroke))
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(event.clone())));
    Tooltip::text(SharedString::from(format!("tip-{id}")), button, cx.t(label))
        .side(Side::Right)
        .into_any_element()
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
        tools.push(tool_button(id, icon, event, size, Side::Right, cx));
    }
    let code_column =
        has_selection.then(|| code_column_toggle("rail-code-column", Side::Right, cx));
    let project_buttons = has_tasks.then(|| {
        [
            icon_button(
                "rail-open-project",
                "folder-plus",
                1.9,
                SidebarEvent::OpenProject,
                "sidebar.openProject",
                cx,
            ),
            super::repo_picker::render(sidebar, super::repo_picker::Place::Rail, cx),
        ]
    });
    // Without tasks the middle offers to open or clone a project.
    let empty_buttons = (!has_tasks).then(|| {
        div()
            .flex()
            .h_full()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .px(px(4.0))
            .child(icon_button(
                "rail-empty-open-project",
                "folder-plus",
                1.9,
                SidebarEvent::OpenProject,
                "sidebar.openProject",
                cx,
            ))
            .child(icon_button(
                "rail-empty-clone",
                "plus",
                2.1,
                SidebarEvent::CloneProject,
                "sidebar.cloneFromUrl",
                cx,
            ))
    });
    let expand = icon_button(
        "expand-sidebar",
        "panel-right",
        1.8,
        SidebarEvent::ToggleCollapsed,
        "sidebar.expandSidebar",
        cx,
    );
    let new_task = icon_button(
        "rail-new-task",
        "plus",
        1.9,
        SidebarEvent::NewTask,
        "sidebar.newWorkspace",
        cx,
    );
    let pull_requests = icon_button(
        "rail-pull-requests",
        "git-pull-request",
        1.9,
        SidebarEvent::OpenPullRequests,
        "sidebar.pullRequests",
        cx,
    );
    let notes = icon_button(
        "rail-notes",
        "sticky-note",
        2.0,
        SidebarEvent::OpenNotes,
        "notes.headingShort",
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
                )
                .children(empty_buttons),
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
    let tile = div()
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
        })));
    Tooltip::text(
        SharedString::from(format!("tip-rail-task-{task_id}")),
        tile,
        title.to_string(),
    )
    .side(Side::Right)
    .into_any_element()
}
