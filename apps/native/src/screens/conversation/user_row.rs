//! `UserMessage`: the prompt in a bubble on the right (`max-w-[75%]`,
//! `rounded-xl border-border/45 bg-accent/35 px-3.5 py-3`), its attachments
//! as chips above the text, and the time under it.

use dcc_core::domain::timeline::TimelineMessage;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px, relative};

use super::hover_actions::{MESSAGE_GROUP, copy_action};
use super::{ConversationView, time_ago};
use crate::data::prompt::{PromptPart, basename, is_audio_path, is_image_path, prompt_parts};
use crate::i18n::Translate;
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// `fileMentionMarkdown`: a mention reads as `@name`, linked to its path.
fn mention_markdown(path: &str) -> String {
    let name: String = basename(path)
        .chars()
        .flat_map(|c| match c {
            '\\' | '[' | ']' | '*' | '_' | '`' => vec!['\\', c],
            _ => vec![c],
        })
        .collect();
    format!("[@{name}](<./{path}>)")
}

fn attachment_chip(path: &str, cx: &gpui::App) -> AnyElement {
    let colors = &cx.theme().colors;
    let icon = if is_image_path(path) {
        "file-image"
    } else if is_audio_path(path) {
        "file-headphone"
    } else {
        "file"
    };
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .max_w_full()
        .min_w_0()
        .rounded(cx.theme().radius.lg)
        .border_1()
        .border_color(colors.border.opacity(0.6))
        .bg(colors.background.opacity(0.7))
        .px(px(8.0))
        .py(px(4.0))
        .text_size(px(11.0))
        .line_height(px(16.0))
        .text_color(colors.muted_foreground)
        .child(
            Icon::new(icon)
                .size(14.0)
                .color(colors.foreground.opacity(0.65)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(SharedString::from(basename(path).to_string())),
        )
        .into_any_element()
}

pub(super) fn user_row(
    view: &ConversationView,
    message: &TimelineMessage,
    window: &mut Window,
    cx: &mut Context<ConversationView>,
) -> AnyElement {
    let parts = prompt_parts(&message.content);
    let attachments: Vec<&str> = parts
        .iter()
        .filter_map(|part| match part {
            PromptPart::Attachment(path) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let markdown: String = parts
        .iter()
        .map(|part| match part {
            PromptPart::Text(text) => text.clone(),
            PromptPart::Mention(path) => mention_markdown(path),
            PromptPart::Attachment(_) => String::new(),
        })
        .collect::<String>()
        .trim()
        .to_string();
    let theme = cx.theme();
    let colors = theme.colors.clone();
    let radius = theme.radius;
    let text = (!markdown.is_empty()).then(|| {
        let blocks = view.markdown(&markdown);
        div()
            .w_full()
            .min_w_0()
            .text_color(colors.foreground)
            .text_size(px(13.0))
            .child(render_markdown(
                &blocks,
                MarkdownStyle::prompt(),
                &message.id,
                window,
                cx,
            ))
    });
    let chips = (!attachments.is_empty()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap(px(6.0))
            .mb(px(8.0))
            .children(attachments.iter().map(|path| attachment_chip(path, cx)))
    });
    let retry = message.retry_of_turn_id.as_ref().map(|_| {
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .rounded_full()
            .border_1()
            .border_color(colors.border.opacity(0.6))
            .px(px(6.0))
            .text_size(px(10.0))
            .line_height(px(16.0))
            .text_color(colors.muted_foreground)
            .child(Icon::new("rotate-ccw").size(12.0))
            .child(cx.t("conversation.message.retryChip"))
    });
    let stamp = time_ago(view, message.created_at.as_deref(), cx);
    div()
        .flex()
        .min_w_0()
        .justify_end()
        .group(MESSAGE_GROUP)
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .items_end()
                .max_w(relative(0.75))
                .min_w_0()
                .pb(px(20.0))
                .child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .rounded(radius.xl)
                        .border_1()
                        .border_color(colors.border.opacity(0.45))
                        .bg(colors.accent.opacity(0.35))
                        .px(px(14.0))
                        .py(px(12.0))
                        .line_height(px(28.0))
                        .children(chips)
                        .children(text),
                )
                .child(
                    div()
                        .mt(px(4.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(colors.muted_foreground)
                        .children(retry)
                        .children(stamp),
                )
                .child(copy_action(
                    SharedString::from(format!("{}-copy", message.id)),
                    message.content.clone(),
                    cx,
                )),
        )
        .into_any_element()
}
