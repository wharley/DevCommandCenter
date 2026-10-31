//! `SidebarChildTaskRow`: a task the agent of a row proposed or started,
//! hanging under it on a guide line. Green while it works, grey while it is
//! stopped, amber when it waits on the person.

use std::time::Duration;

use gpui::{AnyElement, App, ClickEvent, Context, SharedString, Window, div, prelude::*, px};

use super::{Sidebar, SidebarEvent};
use crate::data::child_tasks::{ChildTaskState, live_state};
use crate::i18n::Translate;
use crate::project::ProviderIcon;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};

/// `animate-pulse`: opacity 1 → 0.5 → 1 every 2s.
fn pulse(dot: gpui::Div, id: SharedString, cx: &App) -> AnyElement {
    crate::ui::animated(
        dot,
        id,
        crate::ui::looping(Duration::from_secs(2)),
        cx,
        |dot, delta| {
            let wave = (delta * std::f32::consts::TAU).cos();
            dot.opacity(0.75 + 0.25 * wave)
        },
    )
}

pub fn render(
    sidebar: &Sidebar,
    task_id: &str,
    child_id: &str,
    last: bool,
    cx: &mut Context<Sidebar>,
) -> AnyElement {
    let Some(child) = sidebar
        .state
        .data
        .child_tasks
        .get(task_id)
        .and_then(|children| children.iter().find(|child| child.id == child_id))
    else {
        return div().into_any_element();
    };
    // Live state is read here, not in the list: agents update every few
    // seconds and the list must not change (or scroll) with them.
    let (running, waiting) = sidebar
        .state
        .data
        .activities
        .get(task_id)
        .map(|activity| {
            (
                activity.running_session_ids.clone(),
                activity.waiting_session_ids.clone(),
            )
        })
        .unwrap_or_default();
    let state = live_state(child, &running, &waiting);
    let selected = sidebar.state.selected_task_id.as_deref() == Some(task_id)
        && sidebar.selected_session_id.as_deref() == Some(child.session_id.as_str())
        && state != ChildTaskState::Proposed;
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let dark = theme.appearance == Appearance::Dark;
    let amber_text = if dark {
        tw::AMBER_300.hsla()
    } else {
        tw::AMBER_700.hsla()
    };
    let hover = colors.accent.opacity(0.6);
    let hover_text = colors.foreground;

    let dot = div().size(px(6.0)).flex_shrink_0().rounded_full();
    let dot_id = SharedString::from(format!("child-dot-{child_id}"));
    let dot = match state {
        ChildTaskState::Working => pulse(dot.bg(tw::EMERALD_500.hsla()), dot_id, cx),
        ChildTaskState::Idle => dot
            .bg(colors.muted_foreground.opacity(0.4))
            .into_any_element(),
        ChildTaskState::Review => dot.bg(tw::AMBER_500.hsla()).into_any_element(),
        ChildTaskState::Waiting => pulse(dot.bg(tw::AMBER_500.hsla()), dot_id, cx),
        ChildTaskState::Proposed => dot
            .border_1()
            .border_color(tw::AMBER_500.hsla())
            .into_any_element(),
    };
    let parent = task_id.to_string();
    let session = child.session_id.clone();
    div()
        .id(SharedString::from(format!("child-task-{child_id}")))
        .relative()
        .flex()
        .size_full()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .pl(px(44.0))
        .pr(px(12.0))
        .rounded(theme.radius.md)
        .cursor_pointer()
        .when(selected, |row| {
            row.bg(colors.accent).text_color(colors.foreground)
        })
        .when(!selected, |row| row.text_color(colors.muted_foreground))
        .hover(move |style| style.bg(hover).text_color(hover_text))
        .on_click(cx.listener(move |_, _: &ClickEvent, _: &mut Window, cx| {
            cx.emit(SidebarEvent::OpenSession {
                task_id: parent.clone(),
                session_id: session.clone(),
            })
        }))
        // The guide from under the row's glyph down through its tasks, a tick to each.
        .child(
            div()
                .absolute()
                .left(px(30.0))
                .top_0()
                .w(px(1.0))
                .when(last, |line| line.h(gpui::relative(0.5)))
                .when(!last, |line| line.h_full())
                .bg(colors.border),
        )
        .child(
            div()
                .absolute()
                .left(px(30.0))
                .top(gpui::relative(0.5))
                .h(px(1.0))
                .w(px(10.0))
                .bg(colors.border),
        )
        .child(dot)
        .when_some(child.provider_id.clone(), |row, provider| {
            row.child(
                div()
                    .flex_shrink_0()
                    .opacity(0.75)
                    .child(ProviderIcon::new(&provider).size(11.0)),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .line_height(px(20.0))
                .child(SharedString::from(child.title.clone())),
        )
        .when(state.waits_on_person(), |row| {
            row.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(10.5))
                    .line_height(px(20.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(amber_text)
                    .child(cx.t(state.i18n_key())),
            )
        })
        .into_any_element()
}
