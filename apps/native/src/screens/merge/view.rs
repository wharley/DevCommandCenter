//! The resolver's frame: the amber header (the mark, "Resolver conflitos",
//! the workspace ← the base, the files' progress, "Abortar merge", ×) and
//! what is under it — reading git, an error, an operation it doesn't
//! handle, "Iniciar resolução", the completion with the project's checks,
//! or the files and the open one.

use dcc_core::domain::merge_conflict::WorkspaceGitConflictOperation;
use gpui::{
    AnyElement, Context, FontWeight, Hsla, Render, SharedString, Window, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use super::{Busy, Load, MergeResolver};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Tracked, spinner};

pub(crate) fn dark(cx: &gpui::App) -> bool {
    cx.theme().appearance == Appearance::Dark
}

/// The files' progress bar (`h-1 rounded-full`, emerald).
pub(crate) fn progress(resolved: usize, total: usize, cx: &gpui::App) -> AnyElement {
    let share = if total == 0 {
        0.0
    } else {
        resolved as f32 / total as f32
    };
    div()
        .h(px(4.0))
        .w_full()
        .overflow_hidden()
        .rounded_full()
        .bg(cx.theme().colors.muted)
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(tw::EMERALD_500.hsla())
                .w(gpui::relative(share)),
        )
        .into_any_element()
}

fn chip(text: String, border: Hsla, background: Hsla, foreground: Hsla) -> AnyElement {
    div()
        .max_w(px(160.0))
        .truncate()
        .rounded(px(6.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .px(px(6.0))
        .py(px(2.0))
        .font_family(fonts::MONO)
        .text_color(foreground)
        .child(text)
        .into_any_element()
}

fn header(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let dark = dark(cx);
    let ((current, _), (incoming, _)) = resolver.labels(cx);
    let (resolved, total) = resolver.resolved();
    let merging = matches!(&resolver.state, Load::Ready(state) if state.operation == WorkspaceGitConflictOperation::Merge);
    let busy = resolver.busy.is_some();
    let amber_text = if dark { tw::AMBER_200 } else { tw::AMBER_800 }.hsla();
    div()
        .relative()
        .flex_shrink_0()
        .border_b_1()
        .border_color(colors.border.opacity(0.6))
        .bg(linear_gradient(
            90.0,
            linear_color_stop(tw::AMBER_500.alpha(0.08), 0.0),
            linear_color_stop(colors.background, 0.5),
        ))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .size(px(40.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(tw::AMBER_500.alpha(0.2))
                        .bg(tw::AMBER_500.alpha(0.12))
                        .text_color(if dark { tw::AMBER_300 } else { tw::AMBER_700 }.hsla())
                        .shadow_sm()
                        .child(Icon::new("git-merge").size(20.0)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(14.0))
                                .line_height(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(Tracked::new(cx.t("mergeConflict.title"), 14.0, -0.01)),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .flex()
                                .min_w_0()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(11.0))
                                .text_color(colors.muted_foreground)
                                .child(chip(
                                    current,
                                    colors.border.opacity(0.6),
                                    colors.background.opacity(0.7),
                                    colors.foreground.opacity(0.8),
                                ))
                                .child("←")
                                .child(chip(
                                    incoming,
                                    tw::AMBER_500.alpha(0.2),
                                    tw::AMBER_500.alpha(0.08),
                                    amber_text,
                                )),
                        ),
                )
                .when(total > 0, |row| {
                    row.child(
                        div()
                            .min_w(px(144.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(colors.border.opacity(0.5))
                            .bg(colors.background.opacity(0.6))
                            .px(px(12.0))
                            .py(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(10.0))
                                    .text_color(colors.muted_foreground)
                                    .child(cx.t("mergeConflict.files"))
                                    .child(
                                        div()
                                            .font_family(fonts::MONO)
                                            .text_color(colors.foreground)
                                            .child(format!("{resolved}/{total}")),
                                    ),
                            )
                            .child(div().mt(px(6.0)).child(progress(resolved, total, cx))),
                    )
                })
                .when(merging, |row| {
                    row.child(
                        Button::new("merge-abort")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .disabled(busy)
                            .on_click(cx.listener(|resolver, _, _, cx| resolver.request_abort(cx)))
                            .child(Icon::new("rotate-ccw").size(14.0))
                            .child(cx.t("mergeConflict.abort")),
                    )
                })
                .child(
                    Button::new("merge-close")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .disabled(busy)
                        .on_click(cx.listener(|resolver, _, _, cx| resolver.request_close(cx)))
                        .child(Icon::new("x").size(16.0)),
                ),
        )
        .into_any_element()
}

pub(crate) fn centered(content: impl IntoElement, max_width: f32) -> AnyElement {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .p(px(32.0))
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .w_full()
                .max_w(px(max_width))
                .child(content),
        )
        .into_any_element()
}

pub(crate) fn title(text: SharedString, top: f32) -> AnyElement {
    div()
        .mt(px(top))
        .text_size(px(16.0))
        .line_height(px(24.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_center()
        .child(text)
        .into_any_element()
}

pub(crate) fn note(text: SharedString, cx: &gpui::App) -> AnyElement {
    div()
        .mt(px(8.0))
        .text_size(px(14.0))
        .line_height(px(24.0))
        .text_color(cx.theme().colors.muted_foreground)
        .text_center()
        .child(text)
        .into_any_element()
}

fn start(resolver: &MergeResolver, cx: &mut Context<MergeResolver>) -> AnyElement {
    let ((current, _), (incoming, _)) = resolver.labels(cx);
    let busy = resolver.busy;
    centered(
        div()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .text_color(tw::AMBER_500.hsla())
                    .child(Icon::new("git-merge").size(40.0)),
            )
            .child(title(
                cx.t_with(
                    "mergeConflict.start.title",
                    &Args::default()
                        .with("incoming", incoming)
                        .with("current", current),
                ),
                16.0,
            ))
            .child(note(cx.t("mergeConflict.start.description"), cx))
            .child(
                div().mt(px(20.0)).child(
                    Button::new("merge-start")
                        .size(ButtonSize::Default)
                        .disabled(busy.is_some() || resolver.request.task_id.is_none())
                        .on_click(cx.listener(|resolver, _, _, cx| resolver.start_merge(cx)))
                        .child(if busy == Some(Busy::Start) {
                            spinner("merge-start-busy", 16.0).into_any_element()
                        } else {
                            Icon::new("git-merge").size(16.0).into_any_element()
                        })
                        .child(cx.t("mergeConflict.start.action")),
                ),
            ),
        512.0,
    )
}

impl Render for MergeResolver {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors.clone();
        let body = match &self.state {
            Load::Pending => centered(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_color(colors.muted_foreground)
                    .child(spinner("merge-loading", 16.0))
                    .child(cx.t("mergeConflict.loadingGit")),
                448.0,
            ),
            Load::Failed(error) => centered(
                div().text_color(colors.destructive).child(error.clone()),
                448.0,
            ),
            Load::Ready(state) => match state.operation {
                WorkspaceGitConflictOperation::None if state.conflicts.is_empty() => {
                    start(self, cx)
                }
                WorkspaceGitConflictOperation::None | WorkspaceGitConflictOperation::Merge
                    if state.conflicts.is_empty() =>
                {
                    super::completion_view::completion(self, cx)
                }
                WorkspaceGitConflictOperation::None | WorkspaceGitConflictOperation::Merge => {
                    super::workspace_view::workspace(self, cx)
                }
                other => {
                    let name = serde_json::to_value(other)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                        .unwrap_or_default();
                    centered(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .text_color(tw::AMBER_500.hsla())
                                    .child(Icon::new("triangle-alert").size(32.0)),
                            )
                            .child(title(
                                cx.t_with(
                                    "mergeConflict.unsupported.title",
                                    &Args::default().with("operation", name),
                                ),
                                12.0,
                            ))
                            .child(note(cx.t("mergeConflict.unsupported.description"), cx)),
                        448.0,
                    )
                }
            },
        };
        div()
            .debug_selector(|| "merge-resolver".into())
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header(self, cx))
            .child(body)
            .children(super::confirm::confirm_dialog(self, cx))
    }
}
