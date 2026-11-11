//! How a review comment's text shows: its severity marker as a small chip
//! (red for "Bloqueante", amber for "Atenção", neutral for "Reuso", the
//! primary tint for a suggestion), the comment whole as markdown on the
//! stage's drawer, and one plain line in the lists (the bar over the diff,
//! "Conversas do PR" and the callout on the diff's line).

use dcc_core::domain::review_threads::WorkspacePrReviewComment;
use gpui::{AnyElement, App, FontWeight, Hsla, SharedString, Window, div, prelude::*, px};

use crate::data::review_comment_text::{Severity, comment_lead, comment_preview};
use crate::i18n::Translate;
use crate::markdown::{MarkdownStyle, parse, render_markdown};
use crate::theme::tailwind as tw;
use crate::theme::{ActiveTheme, Appearance};

/// The chip's text and fill for `severity`.
fn severity_colors(severity: Severity, cx: &App) -> (Hsla, Hsla) {
    let colors = &cx.theme().colors;
    let dark = cx.theme().appearance == Appearance::Dark;
    match severity {
        Severity::Blocking => (colors.destructive, colors.destructive.opacity(0.1)),
        Severity::Warning => (
            if dark { tw::AMBER_400 } else { tw::AMBER_700 }.hsla(),
            tw::AMBER_500.alpha(0.12),
        ),
        Severity::Reuse => (colors.muted_foreground, colors.muted),
        Severity::Suggestion => (colors.primary, colors.primary.opacity(0.1)),
    }
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Blocking => "reviewThread.severity.blocking",
        Severity::Warning => "reviewThread.severity.warning",
        Severity::Reuse => "reviewThread.severity.reuse",
        Severity::Suggestion => "reviewThread.severity.suggestion",
    }
}

/// The marker as a chip: its own words, else the severity's name.
pub(crate) fn severity_chip(
    selector: &'static str,
    severity: Severity,
    label: Option<&str>,
    size: f32,
    cx: &App,
) -> AnyElement {
    let (text, fill) = severity_colors(severity, cx);
    let label = label
        .map(|label| SharedString::from(label.to_string()))
        .unwrap_or_else(|| cx.t(severity_name(severity)));
    div()
        .debug_selector(move || selector.into())
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(4.0))
        .rounded_full()
        .px(px(6.0))
        .bg(fill)
        .text_color(text)
        .text_size(px(size))
        .line_height(px(size + 6.0))
        .font_weight(FontWeight::MEDIUM)
        .child(div().size(px(5.0)).rounded_full().bg(text))
        .child(label)
        .into_any_element()
}

/// A comment on one line: the marker's chip, then its first plain words,
/// muted and cut with an ellipsis. It fills its row (`flex_initial` to
/// size it to its text instead).
pub(crate) fn comment_preview_line(body: &str, size: f32, cx: &App) -> gpui::Div {
    let preview = comment_preview(body);
    let colors = &cx.theme().colors;
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .children(preview.severity.map(|severity| {
            severity_chip(
                "review-preview-chip",
                severity,
                preview.label.as_deref(),
                size - 1.0,
                cx,
            )
        }))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(colors.muted_foreground)
                .child(SharedString::from(preview.text)),
        )
}

/// `@login`, or "Revisor" when the forge did not say.
pub(crate) fn author_label(comment: &WorkspacePrReviewComment, cx: &App) -> SharedString {
    comment
        .author
        .as_ref()
        .and_then(|author| author.login.clone())
        .map(|login| SharedString::from(format!("@{login}")))
        .unwrap_or_else(|| cx.t("reviewThread.reviewer"))
}

/// `reviewAuthorInitials`
fn initials(login: Option<&str>) -> String {
    match login.map(str::trim).filter(|login| !login.is_empty()) {
        Some(login) => login.chars().take(2).collect::<String>().to_uppercase(),
        None => "?".into(),
    }
}

/// The comment's text: its marker as a chip, the rest as markdown (bold,
/// code, links, lists), wrapping.
pub(crate) fn comment_body(body: &str, id: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let lead = comment_lead(body);
    let chip = lead.severity.map(|severity| {
        severity_chip(
            "review-severity-chip",
            severity,
            lead.label.as_deref(),
            11.0,
            cx,
        )
    });
    let blocks = parse(&lead.body);
    let text = (!blocks.is_empty())
        .then(|| render_markdown(&blocks, MarkdownStyle::review(), id, window, cx));
    div()
        .debug_selector(|| "review-comment-body".into())
        .flex()
        .flex_col()
        .items_start()
        .gap(px(6.0))
        .min_w_0()
        .text_color(cx.theme().colors.foreground.opacity(0.9))
        .children(chip)
        .children(text.map(|text| div().w_full().min_w_0().overflow_hidden().child(text)))
        .into_any_element()
}

/// One comment of a conversation: the avatar, `@author` and when, then its
/// text.
pub(crate) fn comment_entry(
    comment: &WorkspacePrReviewComment,
    now: i64,
    english: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let login = comment
        .author
        .as_ref()
        .and_then(|author| author.login.as_deref());
    let time = comment
        .created_at
        .as_deref()
        .and_then(crate::data::time::parse_ms)
        .map(|at| crate::data::time::relative_time(at, now, english));
    let author = author_label(comment, cx);
    let body = comment_body(
        &comment.body,
        &format!("review-comment-{}", comment.id),
        window,
        cx,
    );
    div()
        .relative()
        .pl(px(36.0))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .flex()
                .size(px(24.0))
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(colors.muted)
                .border_2()
                .border_color(colors.popover)
                .text_size(px(9.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.muted_foreground)
                .child(initials(login)),
        )
        .child(
            div()
                .flex()
                .h(px(24.0))
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                        .child(author),
                )
                .children(time.map(|time| {
                    div()
                        .flex_shrink_0()
                        .text_size(px(11.0))
                        .text_color(colors.muted_foreground)
                        .child(time)
                })),
        )
        .child(div().mt(px(4.0)).child(body))
        .into_any_element()
}
