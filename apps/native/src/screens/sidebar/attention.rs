//! "Precisa de você": tasks blocked on the person or with a result not
//! opened yet, newest return first, five at a time.

use dcc_core::domain::attention::AttentionItem;
use gpui::{AnyElement, ClickEvent, Context, Window, div, prelude::*, px};

use super::Sidebar;
use super::labels::{activity_time, reason_dot, reason_label, reason_text};
use crate::data::tasks::path_leaf;
use crate::i18n::{Args, Translate};
use crate::project::ProjectGlyph;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::{Tracked, tabular_nums};

pub fn render(sidebar: &Sidebar, cx: &mut Context<Sidebar>) -> Option<AnyElement> {
    let (visible, hidden) = sidebar.state.shown_attention();
    if visible.is_empty() {
        return None;
    }
    let total = visible.len() + hidden;
    let theme = cx.theme();
    let pill_text = match theme.appearance {
        Appearance::Light => tw::AMBER_700.hsla(),
        Appearance::Dark => tw::AMBER_300.hsla(),
    };
    let heading = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .mb(px(4.0))
        .px(px(4.0))
        .child(
            div()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.colors.muted_foreground.opacity(0.8))
                .child(Tracked::new(
                    cx.t("sidebar.attention.title").to_uppercase(),
                    10.0,
                    0.08,
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(16.0))
                .min_w(px(16.0))
                .px(px(4.0))
                .rounded_full()
                .bg(tw::AMBER_500.alpha(0.12))
                .text_size(px(9.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .font_features(tabular_nums())
                .text_color(pill_text)
                .child(total.to_string()),
        );
    let rows: Vec<AnyElement> = visible
        .iter()
        .map(|item| attention_row(sidebar, item, cx).into_any_element())
        .collect();
    let toggle = if hidden > 0 {
        Some(more_button(
            cx.t_with("sidebar.attention.showMore", &Args::count(hidden as i64)),
            true,
            cx,
        ))
    } else if sidebar.state.show_all_attention && total > super::state::ATTENTION_PREVIEW_LIMIT {
        Some(more_button(cx.t("sidebar.attention.showFewer"), false, cx))
    } else {
        None
    };
    let theme = cx.theme();
    Some(
        div()
            .flex_shrink_0()
            .px(px(8.0))
            .pb(px(12.0))
            .child(heading)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .p(px(2.0))
                    .rounded(theme.radius.lg)
                    .border_1()
                    .border_color(theme.colors.border.opacity(0.45))
                    .bg(theme.colors.foreground.opacity(0.018))
                    .children(rows)
                    .children(toggle),
            )
            .into_any_element(),
    )
}

/// The project a "Needs you" row names: its coordinated projects, else its own.
fn project_label(sidebar: &Sidebar, item: &AttentionItem, cx: &Context<Sidebar>) -> String {
    let Some(task) = sidebar.state.task(&item.workspace_id) else {
        return String::new();
    };
    let mut members: Vec<&str> = Vec::new();
    for name in &task.member_project_names {
        if !members.contains(&name.as_str()) {
            members.push(name);
        }
    }
    if members.len() > 1 {
        return members.join(" · ");
    }
    sidebar
        .project_for(task)
        .map(|project| project.display_name.clone())
        .or_else(|| path_leaf(task.project_key()))
        .or_else(|| members.first().map(|name| name.to_string()))
        .unwrap_or_else(|| cx.t("sidebar.unknownProject").to_string())
}

fn attention_row(
    sidebar: &Sidebar,
    item: &AttentionItem,
    cx: &mut Context<Sidebar>,
) -> impl IntoElement {
    let task_id = item.workspace_id.clone();
    let title = sidebar
        .state
        .task(&task_id)
        .map(|task| task.display_title())
        .unwrap_or_default();
    let project = sidebar
        .state
        .task(&task_id)
        .and_then(|task| sidebar.project_for(task))
        .cloned();
    let activity = sidebar.state.data.activities.get(&task_id);
    let time =
        activity.and_then(|activity| activity_time(activity, true, sidebar.state.now_ms, cx));
    let reason = reason_label(item.reason, item.count, cx);
    let dot = reason_dot(item.reason, cx);
    let reason_color = reason_text(item.reason, cx);
    let label = project_label(sidebar, item, cx);
    let selected = sidebar.state.selected_task_id.as_deref() == Some(task_id.as_str());
    let theme = cx.theme();
    let colors = &theme.colors;
    let hover = colors.accent.opacity(0.6);
    let hover_text = colors.foreground;
    let selected_background = super::row::selected_background(cx);
    div()
        .id(gpui::SharedString::from(format!("attention-{task_id}")))
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(44.0))
        .w_full()
        .min_w_0()
        .px(px(8.0))
        .rounded(theme.radius.md)
        .cursor_pointer()
        .when(selected, |row| {
            row.bg(selected_background)
                .text_color(colors.sidebar_foreground)
        })
        .when(!selected, |row| {
            row.text_color(colors.foreground.opacity(0.85))
                .hover(move |style| style.bg(hover).text_color(hover_text))
        })
        // The desktop opens the waiting conversation (maybe a child's); that
        // choice moves here with the conversation screen.
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| {
                this.open_task(&task_id, cx);
            }),
        )
        .when_some(project, |row, project| {
            row.child(
                ProjectGlyph::new(
                    project.icon.as_deref(),
                    project.color.as_deref(),
                    Some(&project.root_path),
                )
                .tile(18.0),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(px(10.0))
                        .line_height(px(16.0))
                        .child(div().size(px(6.0)).flex_shrink_0().rounded_full().bg(dot))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(reason_color)
                                .child(reason),
                        )
                        .child(
                            div()
                                .text_color(colors.muted_foreground.opacity(0.5))
                                .child("·"),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_color(colors.muted_foreground)
                                .child(label),
                        ),
                ),
        )
        .when_some(time, |row, time| {
            row.child(
                div()
                    .mt(px(6.0))
                    .flex_shrink_0()
                    .self_start()
                    .whitespace_nowrap()
                    .text_size(px(10.0))
                    .line_height(px(16.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .font_features(tabular_nums())
                    .text_color(colors.muted_foreground)
                    .child(time),
            )
        })
}

fn more_button(label: gpui::SharedString, show_all: bool, cx: &mut Context<Sidebar>) -> AnyElement {
    let theme = cx.theme();
    let hover = theme.colors.accent.opacity(0.5);
    let hover_text = theme.colors.foreground;
    div()
        .id(if show_all {
            "attention-more"
        } else {
            "attention-fewer"
        })
        .flex()
        .items_center()
        .justify_center()
        .h(px(28.0))
        .w_full()
        .rounded(theme.radius.md)
        .cursor_pointer()
        .text_size(px(10.5))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(theme.colors.muted_foreground)
        .hover(move |style| style.bg(hover).text_color(hover_text))
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| {
                this.state.show_all_attention = show_all;
                cx.notify();
            }),
        )
        .child(label)
        .into_any_element()
}
