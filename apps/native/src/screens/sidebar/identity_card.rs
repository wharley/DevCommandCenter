//! `WorkspaceIdentityCard`: what a task is, shown on hover — project,
//! repository, branch against its base, worktree or local checkout, context
//! folders and coordinated projects.

use gpui::{AnyElement, App, SharedString, div, prelude::*, px};

use super::labels::{activity_time, elapsed_since};
use crate::data::activity::TaskActivity;
use crate::data::tasks::{Project, TaskSummary, path_leaf};
use crate::i18n::{Args, Translate};
use crate::project::ProjectGlyph;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};
use crate::ui::Icon;

pub struct CardInput<'a> {
    pub task: &'a TaskSummary,
    pub project: Option<&'a Project>,
    pub activity: Option<&'a TaskActivity>,
    pub current_branch: &'a str,
    pub now_ms: i64,
}

fn line(icon: &'static str, color: Option<gpui::Hsla>) -> gpui::Div {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .when_some(color, |line, color| line.text_color(color))
        .child(Icon::new(icon).size(14.0).stroke(1.8))
}

pub fn render(input: CardInput, cx: &App) -> AnyElement {
    let task = input.task;
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    let title = task.display_title();
    let repository = path_leaf(&task.root_path);
    let project_name = input
        .project
        .map(|project| project.display_name.clone())
        .or_else(|| repository.clone());
    let show_repository = match (&repository, &project_name) {
        (Some(repository), Some(project)) => repository.to_lowercase() != project.to_lowercase(),
        _ => false,
    };
    // Without an agent the task's own last update stands in for the time.
    let time = match input.activity {
        Some(activity) => activity_time(activity, true, input.now_ms, cx),
        None => elapsed_since(&task.updated_at, input.now_ms, cx),
    };
    let base = task.base_branch.trim();
    let mut members: Vec<&str> = Vec::new();
    for name in &task.member_project_names {
        if !members.contains(&name.as_str()) {
            members.push(name);
        }
    }
    let worktree = task
        .worktree_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    let (protected, local) = if dark {
        (tw::EMERALD_400.hsla(), tw::AMBER_400.hsla())
    } else {
        (tw::EMERALD_600.hsla(), tw::AMBER_600.hsla())
    };

    let details = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .pt(px(8.0))
        .border_t_1()
        .border_color(colors.border.opacity(0.7))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .when_some(project_name.clone(), |details, name| {
            details.child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    .when_some(input.project, |line, project| {
                        line.child(ProjectGlyph::new(
                            project.icon.as_deref(),
                            project.color.as_deref(),
                            Some(&project.root_path),
                        ))
                    })
                    .child(
                        div()
                            .truncate()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(SharedString::from(name)),
                    ),
            )
        })
        .when(show_repository, |details| {
            details.child(
                line("folder", Some(colors.muted_foreground)).child(
                    div()
                        .truncate()
                        .child(SharedString::from(repository.clone().unwrap_or_default())),
                ),
            )
        })
        .child(
            line("git-branch", Some(colors.muted_foreground))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.popover_foreground.opacity(0.9))
                        .child(SharedString::from(input.current_branch.to_string())),
                )
                .when(!base.is_empty() && base != input.current_branch, |line| {
                    line.child(
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .text_size(px(9.5))
                            .child(cx.t_with(
                                "sidebar.workspaceIdentity.base",
                                &Args::default().with("branch", base),
                            )),
                    )
                }),
        )
        .child(if worktree {
            line("shield-check", Some(protected))
                .child(cx.t("sidebar.workspaceIdentity.protectedWorktree"))
        } else {
            line("folder-open", Some(local)).child(cx.t("sidebar.workspaceIdentity.localCheckout"))
        })
        .when(!task.context_folders.is_empty(), |details| {
            let folders: Vec<String> = task
                .context_folders
                .iter()
                .filter_map(|folder| path_leaf(folder))
                .collect();
            details.child(
                line("folder-open", Some(colors.muted_foreground)).child(
                    div().line_clamp(2).child(
                        cx.t_with(
                            "sidebar.workspaceIdentity.contextFolders",
                            &Args::count(task.context_folders.len() as i64)
                                .with("folders", folders.join(" · ")),
                        ),
                    ),
                ),
            )
        })
        .when(members.len() > 1, |details| {
            details.child(
                line("layers", Some(colors.muted_foreground))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(colors.border.opacity(0.6))
                    .child(div().line_clamp(2).child(cx.t_with(
                        "sidebar.workspaceIdentity.coordinatedProjects",
                        &Args::count(members.len() as i64).with("projects", members.join(" · ")),
                    ))),
            )
        });

    div()
        .w(px(292.0))
        .p(px(2.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(colors.popover_foreground)
                        .child(SharedString::from(title)),
                )
                .when_some(time, |header, time| {
                    header.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.muted_foreground)
                            .child(time),
                    )
                }),
        )
        .child(details)
        .into_any_element()
}
