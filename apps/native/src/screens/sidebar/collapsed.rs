//! The collapsed rail (76px): icon buttons on top, one tile per task with
//! its project's mascot (walking while it runs, a dot when it needs the
//! person), the tools at the bottom.

use std::collections::HashMap;

use dcc_core::domain::attention::AttentionReason;
use gpui::{ClickEvent, Context, SharedString, Window, div, prelude::*, px};

use super::footer::{TOOLS, tool_button};
use super::header::code_column_toggle;
use super::labels::reason_dot;
use super::row::{avatar, selected_background};
use super::{Sidebar, SidebarEvent};
use crate::i18n::Translate;
use crate::project::ProjectGlyph;
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
    // The first reason is the newest, as "Needs you" lists them.
    let mut reasons = HashMap::new();
    for item in sidebar.state.attention() {
        reasons.entry(item.workspace_id).or_insert(item.reason);
    }
    let running = sidebar.state.running_ids();
    let mut tiles = Vec::new();
    for task in &sidebar.state.data.tasks {
        let state = TileState {
            running: running.contains(&task.id),
            reason: reasons.get(&task.id).copied(),
        };
        tiles.push(task_tile(sidebar, task, state, cx));
    }
    let mut tools = Vec::new();
    for (id, icon, event) in TOOLS {
        // The feedback button keeps its full size in the rail too.
        let size = if id == "tool-feedback" {
            ButtonSize::Icon
        } else {
            ButtonSize::IconXs
        };
        tools.push(tool_button(id, icon, event, size, Side::Right, sidebar, cx));
    }
    tools.extend(super::footer::update_button(24.0, Side::Right, cx));
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
        .children(super::agents::rail_buttons(sidebar, cx))
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

/// What a task's tile shows besides its project.
#[derive(Clone, Copy)]
struct TileState {
    running: bool,
    reason: Option<AttentionReason>,
}

fn task_tile(
    sidebar: &Sidebar,
    task: &crate::data::tasks::TaskSummary,
    state: TileState,
    cx: &mut Context<Sidebar>,
) -> gpui::AnyElement {
    let task_id = task.id.as_str();
    let title = task.display_title();
    let selected = sidebar.state.selected_task_id.as_deref() == Some(task_id);
    // The same face as the task's row in the open list.
    let glyph = match sidebar.project_for(task) {
        Some(project) => ProjectGlyph::new(
            project.icon.as_deref(),
            project.color.as_deref(),
            Some(&project.root_path),
        )
        .tile(26.0)
        .moving(state.running && sidebar.mascot_frame)
        .into_any_element(),
        None => {
            let provider = sidebar.state.data.provider_ids.get(task_id);
            avatar(&title, &task.name, provider.map(String::as_str), cx).into_any_element()
        }
    };
    let dot = state.reason.map(|reason| {
        div()
            .debug_selector(|| format!("rail-task-dot-{task_id}"))
            .absolute()
            .top(px(1.0))
            .right(px(1.0))
            .size(px(9.0))
            .rounded_full()
            .border_2()
            .border_color(cx.theme().colors.sidebar)
            .bg(reason_dot(reason, cx))
    });
    let theme = cx.theme();
    let colors = &theme.colors;
    let hover_background = colors.accent.opacity(0.6);
    let id = task_id.to_string();
    let tile = div()
        .id(SharedString::from(format!("rail-task-{task_id}")))
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(36.0))
        .rounded(theme.radius.lg)
        .border_1()
        .cursor_pointer()
        .when(selected, |tile| {
            tile.bg(selected_background(cx)).border_color(colors.border)
        })
        .when(!selected, |tile| {
            tile.border_color(gpui::transparent_black())
                .hover(move |style| style.bg(hover_background))
        })
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| this.open_task(&id, cx)),
        )
        .child(glyph)
        .children(dot);
    Tooltip::text(
        SharedString::from(format!("tip-rail-task-{task_id}")),
        tile,
        title,
    )
    .side(Side::Right)
    .into_any_element()
}
