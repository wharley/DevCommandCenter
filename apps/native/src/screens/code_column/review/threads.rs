//! `WorkspaceReviewThreadsSection`: the pull request's conversations across
//! every file under one header (open · outdated · resolved), the picked ones
//! handed to the agent in one editable request; and the badge a file's card
//! and row carry for its conversations still open.

use dcc_core::domain::review_threads::{
    ReviewThread, ReviewThreadState, build_review_threads, build_review_threads_prompt,
    review_comment_line, review_comments_by_path, review_thread_root, review_thread_state,
    sort_review_threads_for_list,
};
use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::super::{CodeColumn, CodeColumnEvent};
use crate::i18n::{Args, Translate};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance, fonts};
use crate::ui::tooltip::Side;
use crate::ui::{Checkbox, Icon, Tooltip, tabular_nums};

impl CodeColumn {
    pub(crate) fn toggle_threads(&mut self, cx: &mut Context<Self>) {
        self.review.threads_open = !self.review.threads_open;
        cx.notify();
    }

    pub(crate) fn toggle_thread_pick(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.review.picked.remove(id) {
            self.review.picked.insert(id.to_string());
        }
        cx.notify();
    }

    /// "Pedir ao agente (N)": the picked conversations, one request.
    pub(crate) fn ask_agent_about_threads(&mut self, cx: &mut Context<Self>) {
        let Some(output) = &self.review.comments else {
            return;
        };
        let picked: Vec<ReviewThread> = threads_in_order(&output.comments)
            .into_iter()
            .filter(|thread| self.review.picked.contains(&thread.id))
            .collect();
        if picked.is_empty() {
            return;
        }
        let url = output
            .url
            .clone()
            .or_else(|| self.delivery.pr.as_ref().and_then(|pr| pr.url.clone()));
        let prompt = build_review_threads_prompt(&picked, url.as_deref());
        let delivered = cx.t_with(
            "reviewThread.section.inComposer",
            &Args::count(picked.len() as i64),
        );
        cx.emit(CodeColumnEvent::AskAgent { prompt, delivered });
    }
}

/// The section's order: by file, then open conversations first.
fn threads_in_order(
    comments: &[dcc_core::domain::review_threads::WorkspacePrReviewComment],
) -> Vec<ReviewThread> {
    review_comments_by_path(comments)
        .into_values()
        .flat_map(|comments| sort_review_threads_for_list(build_review_threads(&comments)))
        .collect()
}

/// `ReviewThreadStateChip`
pub(crate) fn state_chip(state: ReviewThreadState, cx: &gpui::App) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let dark = cx.theme().appearance == Appearance::Dark;
    let (background, text, key) = match state {
        ReviewThreadState::Open => (
            tw::AMBER_500.alpha(0.1),
            if dark { tw::AMBER_400 } else { tw::AMBER_700 }.hsla(),
            "reviewThread.unresolved",
        ),
        ReviewThreadState::Resolved => (
            tw::EMERALD_500.alpha(0.1),
            if dark {
                tw::EMERALD_400
            } else {
                tw::EMERALD_600
            }
            .hsla(),
            "reviewThread.resolved",
        ),
        ReviewThreadState::Outdated => (
            colors.muted,
            colors.muted_foreground,
            "reviewThread.outdated",
        ),
    };
    div()
        .flex_shrink_0()
        .rounded_full()
        .px(px(6.0))
        .bg(background)
        .text_color(text)
        .text_size(px(9.5))
        .line_height(px(15.0))
        .font_weight(FontWeight::MEDIUM)
        .child(cx.t(key))
        .into_any_element()
}

fn thread_row(
    column: &CodeColumn,
    thread: &ReviewThread,
    cx: &mut Context<CodeColumn>,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let root = review_thread_root(thread);
    let state = review_thread_state(thread);
    let line = review_comment_line(root)
        .filter(|_| state != ReviewThreadState::Outdated)
        .map_or_else(|| "—".to_string(), |line| format!("L{line}"));
    let author: SharedString = root
        .author
        .as_ref()
        .and_then(|author| author.login.clone())
        .map(|login| SharedString::from(format!("@{login}")))
        .unwrap_or_else(|| cx.t("reviewThread.reviewer"));
    let id = thread.id.clone();
    let checked = column.review.picked.contains(&id);
    let entity = cx.entity().downgrade();
    let hover = colors.accent.opacity(0.5);
    div()
        .id(SharedString::from(format!("thread-{id}")))
        .debug_selector({
            let id = id.clone();
            move || format!("thread-{id}")
        })
        .flex()
        .items_center()
        .gap(px(6.0))
        .rounded(px(6.0))
        .px(px(4.0))
        .hover(move |style| style.bg(hover))
        .when(state == ReviewThreadState::Resolved, |row| row.opacity(0.6))
        .child(
            Checkbox::new(SharedString::from(format!("thread-pick-{id}")), checked)
                .side(12.0)
                .on_toggle(move |_, _, cx| {
                    let _ = entity.update(cx, |column, cx| column.toggle_thread_pick(&id, cx));
                }),
        )
        .child(
            // The conversation opens on its line, in the pull request's diff.
            div()
                .id(SharedString::from(format!("thread-open-{}", thread.id)))
                .debug_selector({
                    let id = thread.id.clone();
                    move || format!("thread-open-{id}")
                })
                .cursor_pointer()
                .on_click({
                    let thread = thread.clone();
                    cx.listener(move |column, _, _, cx| column.open_thread_on_stage(&thread, cx))
                })
                .flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(6.0))
                .py(px(4.0))
                .text_size(px(10.5))
                .child(
                    div()
                        .w(px(32.0))
                        .flex_shrink_0()
                        .font_family(fonts::MONO)
                        .text_size(px(9.5))
                        .font_features(tabular_nums())
                        .text_color(colors.muted_foreground)
                        .child(line),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_weight(FontWeight::MEDIUM)
                        .child(author),
                )
                .child(super::comment_preview_line(&root.body, 10.5, cx))
                .when(state != ReviewThreadState::Open, |row| {
                    row.child(state_chip(state, cx))
                }),
        )
        .into_any_element()
}

pub(crate) fn threads_section(
    column: &CodeColumn,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let pr_number = column.delivery.pr.as_ref().and_then(|pr| pr.number);
    if !column.review.comments_enabled(pr_number) {
        return None;
    }
    let output = column.review.comments.as_ref()?;
    let threads = threads_in_order(&output.comments);
    if threads.is_empty() {
        return None;
    }
    let colors = cx.theme().colors.clone();
    let (mut open, mut outdated, mut resolved) = (0, 0, 0);
    for thread in &threads {
        match review_thread_state(thread) {
            ReviewThreadState::Open => open += 1,
            ReviewThreadState::Outdated => outdated += 1,
            ReviewThreadState::Resolved => resolved += 1,
        }
    }
    let summary = [
        ("reviewThread.bar.open", open),
        ("reviewThread.bar.outdated", outdated),
        ("reviewThread.bar.resolved", resolved),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(key, count)| cx.t_with(key, &Args::count(count as i64)).to_string())
    .collect::<Vec<_>>()
    .join(" · ");
    let picked = threads
        .iter()
        .filter(|thread| column.review.picked.contains(&thread.id))
        .count();
    let expanded = column.review.threads_open;
    let hint = cx.t("reviewThread.section.askAgentHint");
    let ask = Tooltip::new(
        "threads-ask-agent-tip",
        div()
            .id("threads-ask-agent")
            .debug_selector(|| "threads-ask-agent".into())
            .flex()
            .h(px(24.0))
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .px(px(6.0))
            .text_size(px(10.0))
            .font_weight(FontWeight::MEDIUM)
            .map(|button| {
                if picked == 0 {
                    button.opacity(0.5)
                } else {
                    let hover = colors.muted;
                    button
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(
                            cx.listener(|column, _, _, cx| column.ask_agent_about_threads(cx)),
                        )
                }
            })
            .child(Icon::new("bot").size(12.0))
            .child(cx.t_with("reviewThread.section.askAgent", &Args::count(picked as i64))),
        move |_, _| {
            div()
                .max_w(px(260.0))
                .child(hint.clone())
                .into_any_element()
        },
    )
    .side(Side::Bottom);
    let header = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(8.0))
        .px(px(10.0))
        .py(px(8.0))
        .child(
            div()
                .id("threads-toggle")
                .debug_selector(|| "threads-toggle".into())
                .flex()
                .h(px(24.0))
                .flex_1()
                .min_w_0()
                .items_center()
                .gap(px(8.0))
                .ml(px(-4.0))
                .px(px(4.0))
                .cursor_pointer()
                .on_click(cx.listener(|column, _, _, cx| column.toggle_threads(cx)))
                .child(Icon::new("chevron-right").size(14.0).rotate(if expanded {
                    90.0
                } else {
                    0.0
                }))
                .child(Icon::new("message-square").size(14.0).color(colors.primary))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("reviewThread.section.title")),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(9.5))
                        .text_color(colors.muted_foreground)
                        .child(summary),
                ),
        )
        .child(ask);
    let body = expanded.then(|| {
        let groups: Vec<AnyElement> = review_comments_by_path(&output.comments)
            .into_iter()
            .map(|(path, comments)| {
                let (name, folder) = match path.rfind('/') {
                    Some(at) => (path[at + 1..].to_string(), path[..at].to_string()),
                    None => (path.clone(), String::new()),
                };
                let rows: Vec<AnyElement> =
                    sort_review_threads_for_list(build_review_threads(&comments))
                        .iter()
                        .map(|thread| thread_row(column, thread, cx))
                        .collect();
                div()
                    .mb(px(4.0))
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_baseline()
                            .gap(px(6.0))
                            .px(px(4.0))
                            .py(px(2.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(name),
                            )
                            .when(!folder.is_empty(), |line| {
                                line.child(
                                    div()
                                        .truncate()
                                        .text_size(px(9.5))
                                        .text_color(colors.muted_foreground)
                                        .child(folder),
                                )
                            }),
                    )
                    .children(rows)
                    .into_any_element()
            })
            .collect();
        div()
            .id("threads-list")
            .max_h(px(288.0))
            .overflow_y_scroll()
            .border_t_1()
            .border_color(colors.border.opacity(0.4))
            .px(px(6.0))
            .py(px(6.0))
            .children(groups)
    });
    Some(
        div()
            .debug_selector(|| "review-threads".into())
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(px(6.0))
            .border_1()
            .border_color(if open > 0 {
                tw::AMBER_500.alpha(0.25)
            } else {
                colors.border.opacity(0.6)
            })
            .bg(colors.muted.opacity(0.15))
            .child(header)
            .children(body)
            .into_any_element(),
    )
}

/// `ReviewCommentBadge`: a file's conversations still open on the PR; it
/// opens the card's diff on the first of them.
pub(crate) fn review_badge(
    column: &CodeColumn,
    path: &str,
    at: usize,
    cx: &mut Context<CodeColumn>,
) -> Option<AnyElement> {
    let count = *column.review.open_by_path.get(path)?;
    let colors = cx.theme().colors.clone();
    let label = cx.t_with("reviewThread.badge", &Args::count(count as i64));
    let id = SharedString::from(format!("review-badge-{path}"));
    let selector = format!("review-badge-{path}");
    Some(
        Tooltip::new(
            SharedString::from(format!("{id}-tip")),
            div()
                .id(id)
                .debug_selector(move || selector.clone())
                .flex()
                .h(px(16.0))
                .flex_shrink_0()
                .items_center()
                .gap(px(2.0))
                .rounded_full()
                .bg(colors.primary.opacity(0.1))
                .px(px(4.0))
                .text_size(px(9.5))
                .font_weight(FontWeight::SEMIBOLD)
                .font_features(tabular_nums())
                .text_color(colors.primary)
                .cursor_pointer()
                .on_click(cx.listener(move |column, _, _, cx| {
                    cx.stop_propagation();
                    column.open_change_focused(at, true, cx)
                }))
                .child(Icon::new("message-square").size(10.0).stroke(2.0))
                .child(count.to_string()),
            move |_, _| div().child(label.clone()).into_any_element(),
        )
        .into_any_element(),
    )
}
