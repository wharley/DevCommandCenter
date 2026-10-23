//! One task in the list (`WorkspaceRailRowItem`): the project glyph, the
//! title with its markers, and the state line under it.

use gpui::{App, ClickEvent, Context, Hsla, SharedString, Window, div, prelude::*, px};

use super::row_status::{RowContext, status};
use super::{Sidebar, meta_chips};
use crate::data::tasks::path_leaf;
use crate::project::{ProjectGlyph, ProviderIcon};
use crate::theme::ActiveTheme;
use crate::theme::color::mix_oklab;
use crate::ui::{Icon, tabular_nums};

/// `.workspace-row-selected` (`--workspace-sidebar-selected-bg`).
pub fn selected_background(cx: &App) -> Hsla {
    let colors = &cx.theme().colors;
    mix_oklab(colors.sidebar_accent, 0.985, colors.sidebar_foreground)
}

pub fn render(
    sidebar: &Sidebar,
    task_id: &str,
    compact: bool,
    cx: &mut Context<Sidebar>,
) -> gpui::AnyElement {
    let Some(task) = sidebar.state.task(task_id) else {
        return div().into_any_element();
    };
    let activity = sidebar.state.data.activities.get(task_id);
    let project = sidebar.project_for(task).cloned();
    let selected = sidebar.state.selected_task_id.as_deref() == Some(task_id);
    let selectable = task.status.is_selectable();
    let running = activity.is_some_and(|activity| activity.is_running());
    let unseen = sidebar.unseen_results.contains(task_id);
    let title = task.display_title();
    let project_name = project
        .as_ref()
        .map(|project| project.display_name.clone())
        .or_else(|| path_leaf(&task.root_path));
    let provider = sidebar.state.data.provider_ids.get(task_id).cloned();
    let git = sidebar.state.row_git(task);
    let state_line = status(
        &RowContext {
            task,
            activity,
            git: &git,
            pending_reviews: sidebar.state.pending_reviews(task_id),
            unseen_result: unseen,
            compact,
            now_ms: sidebar.state.now_ms,
        },
        cx,
    );
    let chips = git.meta.as_ref().map(|meta| meta_chips::render(meta, cx));
    let member_count = task.member_workspace_ids.len();
    let pinned = task.pinned_at.is_some();

    let theme = cx.theme();
    let colors = &theme.colors;
    let hover = colors.foreground.opacity(0.05);
    let active_background = colors.foreground.opacity(0.08);
    let muted = colors.muted_foreground;
    let id = task_id.to_string();

    let glyph = match &project {
        Some(project) => ProjectGlyph::new(
            project.icon.as_deref(),
            project.color.as_deref(),
            Some(&project.root_path),
        )
        .tile(18.0)
        .moving(running && sidebar.mascot_frame)
        .into_any_element(),
        None => avatar(&title, &task.name, provider.as_deref(), cx).into_any_element(),
    };

    let title_line = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(gpui::FontWeight::MEDIUM)
                .line_height(px(20.0))
                .text_color(colors.foreground)
                .child(title),
        )
        .when(member_count > 1, |line| {
            line.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(2.0))
                    .h(px(16.0))
                    .px(px(6.0))
                    .rounded_full()
                    .bg(colors.foreground.opacity(0.08))
                    .text_size(px(9.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .font_features(tabular_nums())
                    .text_color(muted)
                    .child(Icon::new("layers").size(10.0))
                    .child(member_count.to_string()),
            )
        })
        .when(pinned, |line| {
            line.child(
                Icon::new("pin")
                    .size(12.0)
                    .stroke(1.9)
                    .rotate(-12.0)
                    .color(muted.opacity(0.65)),
            )
        });

    let detail_line = div()
        .mt(px(1.0))
        .flex()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .overflow_hidden()
                .text_size(px(10.5))
                .line_height(px(16.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(muted)
                .when_some(project_name, |line, name| {
                    line.child(
                        div()
                            .min_w(px(40.0))
                            .max_w(gpui::relative(0.45))
                            .flex_shrink(1.0)
                            .truncate()
                            .child(SharedString::from(name)),
                    )
                    .child(div().flex_shrink_0().opacity(0.4).child("·"))
                })
                .when_some(provider, |line, provider| {
                    line.child(ProviderIcon::new(&provider).size(11.0).opacity(0.75))
                })
                .children(state_line),
        )
        .children(chips);

    div()
        .pl(px(12.0))
        .pr(px(4.0))
        .child(
            div()
                .id(SharedString::from(format!("task-{task_id}")))
                .relative()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(theme.radius.control)
                .cursor_pointer()
                .text_size(px(13.0))
                .text_color(if selected {
                    colors.sidebar_foreground
                } else {
                    colors.foreground.opacity(0.8)
                })
                .when(selected, |card| card.bg(active_background))
                .when(!selected, |card| card.hover(move |style| style.bg(hover)))
                .when(!selectable && !selected, |card| card.opacity(0.5))
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _: &mut Window, cx| {
                        this.open_task(&id, cx)
                    }),
                )
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .items_start()
                        .gap(px(8.0))
                        .child(div().mt(px(2.0)).flex_shrink_0().child(glyph))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(title_line)
                                .child(detail_line),
                        ),
                ),
        )
        .into_any_element()
}

/// `WorkspaceRailAvatar`: a task outside any known project shows its
/// provider, or its initials.
fn avatar(title: &str, name: &str, provider: Option<&str>, cx: &App) -> impl IntoElement {
    let colors = &cx.theme().colors;
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.0))
        .rounded(cx.theme().radius.md)
        .bg(colors.accent.opacity(0.7))
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .text_size(px(8.5))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(colors.foreground)
        .map(|tile| match provider {
            Some(provider) => tile.child(ProviderIcon::new(provider).size(14.0)),
            None => tile.child(SharedString::from(initials(if name.is_empty() {
                title
            } else {
                name
            }))),
        })
}

/// `initialsFromWorkspaceLabel`
pub fn initials(label: &str) -> String {
    if label.trim().is_empty() {
        return "WS".into();
    }
    let parts: Vec<&str> = label
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() >= 2 {
        return parts[..2]
            .iter()
            .filter_map(|part| part.chars().next())
            .map(|character| character.to_ascii_uppercase())
            .collect();
    }
    let letters: String = label
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if letters.is_empty() {
        "WS".into()
    } else {
        letters
    }
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_match_the_desktop() {
        assert_eq!(initials("Update Gateway upay"), "UG");
        assert_eq!(initials("dcc"), "DC");
        assert_eq!(initials("  "), "WS");
        assert_eq!(initials("ção"), "O");
    }
}
