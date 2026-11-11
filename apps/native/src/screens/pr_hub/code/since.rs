//! What changed since the person's review, on the code tab: a strip over
//! the files ("Desde sua revisão: 1 commit novo · @dev respondeu no PR ·
//! 3 conversas com o código alterado · 1 com a linha igual", and "Ver
//! resposta" to the summary), the outdated threads of a file kept apart
//! with the code they were written on (never over the new code), and a
//! thread's mark when its line did not change after new commits.

use gpui::{AnyElement, Context, FontWeight, SharedString, div, prelude::*, px};

use super::Tab;
use crate::data::pr_hub::{Detail, Item};
use crate::data::pr_review::File;
use crate::data::pr_since_review::{is_root, line_unchanged, since_review};
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, fonts};
use crate::ui::Icon;

/// How many of a hunk's last lines show the commented code.
const HUNK_TAIL: usize = 4;

/// What a thread's header says about its code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThreadMark {
    Plain,
    Outdated,
    LineUnchanged,
}

impl ThreadMark {
    pub(super) fn of(comment: &crate::data::pr_review::InlineComment, detail: &Detail) -> Self {
        if !is_root(comment, detail) {
            ThreadMark::Plain
        } else if comment.outdated {
            ThreadMark::Outdated
        } else if line_unchanged(comment, detail) {
            ThreadMark::LineUnchanged
        } else {
            ThreadMark::Plain
        }
    }

    /// The badge in the thread's header.
    pub(super) fn badge(self, cx: &gpui::App) -> Option<AnyElement> {
        let (icon, key, color) = match self {
            ThreadMark::Plain => return None,
            ThreadMark::Outdated => (
                "history",
                "pullRequests.code.outdated",
                cx.theme().colors.muted_foreground,
            ),
            ThreadMark::LineUnchanged => (
                "triangle-alert",
                "pullRequests.code.lineUnchanged",
                tw::AMBER_600.hsla(),
            ),
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .text_color(color)
                .child(Icon::new(icon).size(12.0).color(color))
                .child(cx.t(key))
                .into_any_element(),
        )
    }
}

/// The commented code: the last lines of the hunk the forge kept.
fn hunk_tail(hunk: &str) -> Vec<String> {
    let lines: Vec<&str> = hunk
        .lines()
        .filter(|line| !line.starts_with("@@"))
        .collect();
    let start = lines.len().saturating_sub(HUNK_TAIL);
    lines[start..].iter().map(|line| line.to_string()).collect()
}

impl PrHub {
    pub(super) fn since_strip(
        &self,
        item: &Item,
        detail: &Detail,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let me = item.forge_login.as_deref()?;
        let author = item.author.as_ref().map(|actor| actor.login.as_str());
        let since = since_review(detail, me, author)?;
        let colors = cx.theme().colors.clone();
        let mut parts: Vec<SharedString> = Vec::new();
        if since.commits > 0 {
            parts.push(cx.t_with(
                "pullRequests.code.sinceCommits",
                &Args::count(since.commits as i64),
            ));
        }
        if let (Some(author), false) = (author, since.replies.is_empty()) {
            parts.push(cx.t_with(
                "pullRequests.code.sinceReplied",
                &Args::default().with("author", author.to_string()),
            ));
        }
        for (count, key) in [
            (since.thread_replies, "pullRequests.code.sinceThreadReplies"),
            (since.outdated, "pullRequests.code.sinceOutdated"),
            (since.unchanged, "pullRequests.code.sinceUnchanged"),
        ] {
            if count > 0 {
                parts.push(cx.t_with(key, &Args::count(count as i64)));
            }
        }
        let summary = parts.join(" · ");
        let calm = colors.accent_calm;
        Some(
            div()
                .debug_selector(|| "pr-since-review".into())
                .flex()
                .flex_none()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .border_b_1()
                .border_color(calm.opacity(0.2))
                .bg(calm.opacity(0.06))
                .px(px(16.0))
                .py(px(8.0))
                .text_size(px(11.0))
                .child(Icon::new("git-commit-horizontal").size(14.0).color(calm))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(cx.t("pullRequests.code.sinceReview")),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(colors.muted_foreground)
                        .child(summary),
                )
                .when(!since.replies.is_empty(), |strip| {
                    strip.child(
                        div()
                            .id("pr-since-open-reply")
                            .debug_selector(|| "pr-since-open-reply".into())
                            .cursor_pointer()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(calm)
                            .hover(|style| style.underline())
                            .on_click(cx.listener(|hub, _, _, cx| hub.set_tab(Tab::Summary, cx)))
                            .child(cx.t("pullRequests.code.sinceOpenReply")),
                    )
                })
                .into_any_element(),
        )
    }

    /// A file's outdated threads, over its diff: each with the code it was
    /// written on and its actions.
    pub(super) fn outdated_block(
        &self,
        file: &File,
        detail: &Detail,
        width: f32,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let outdated: Vec<_> = detail
            .inline_comments
            .iter()
            .filter(|comment| comment.path == file.path && comment.outdated)
            .collect();
        if outdated.is_empty() {
            return None;
        }
        let threads = outdated
            .iter()
            .filter(|comment| is_root(comment, detail))
            .count();
        let colors = cx.theme().colors.clone();
        let mut cards = Vec::new();
        for comment in outdated {
            let code = is_root(comment, detail)
                .then(|| comment.diff_hunk.as_deref().map(hunk_tail))
                .flatten()
                .filter(|lines| !lines.is_empty());
            if let Some(lines) = code {
                cards.push(
                    div()
                        .mx(px(12.0))
                        .mt(px(8.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.6))
                        .bg(colors.muted.opacity(0.3))
                        .px(px(10.0))
                        .py(px(6.0))
                        .font_family(fonts::MONO)
                        .text_size(px(10.0))
                        .text_color(colors.muted_foreground)
                        .children(lines.into_iter().map(|line| div().truncate().child(line)))
                        .into_any_element(),
                );
            }
            cards.push(self.thread(comment, ThreadMark::of(comment, detail), width, window, cx));
        }
        Some(
            div()
                .debug_selector(|| "pr-outdated-threads".into())
                .flex()
                .flex_col()
                .border_b_1()
                .border_color(colors.border)
                .pb(px(4.0))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .pt(px(10.0))
                        .text_size(px(11.0))
                        .child(Icon::new("history").size(14.0))
                        .child(div().font_weight(FontWeight::MEDIUM).child(cx.t_with(
                            "pullRequests.code.outdatedThreads",
                            &Args::count(threads as i64),
                        )))
                        .child(
                            div()
                                .text_color(colors.muted_foreground)
                                .child(cx.t("pullRequests.code.outdatedHint")),
                        ),
                )
                .children(cards)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::hunk_tail;

    #[test]
    fn the_commented_code_is_the_end_of_its_hunk() {
        let hunk = "@@ -60,3 +60,6 @@\n a\n+b\n+c\n+d\n+e";
        assert_eq!(hunk_tail(hunk), ["+b", "+c", "+d", "+e"]);
        assert!(hunk_tail("@@ -1 +1 @@").is_empty());
    }
}
