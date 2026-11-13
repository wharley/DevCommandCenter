//! The line comment's reach: "−" and "+" take in the lines above it (the
//! same side of the same hunk), and "Sugerir" writes the `suggestion` block
//! with those lines as they are, for the person to edit into the change
//! they propose.

use gpui::{AnyElement, Context, div, prelude::*, px};

use crate::data::pr_review::{PatchRow, parse_patch};
use crate::data::pr_review_range::{max_span, range, suggestion};
use crate::i18n::{Args, Translate};
use crate::screens::pr_hub::PrHub;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

impl PrHub {
    /// The patch rows of a file of the pull request on screen.
    fn rows_of(&self, path: &str) -> Vec<PatchRow> {
        self.review
            .for_id
            .as_deref()
            .and_then(|id| self.code_read(id))
            .and_then(|read| read.as_ref().ok())
            .and_then(|detail| detail.files.iter().find(|file| file.path == path))
            .map(|file| parse_patch(file.patch.as_deref()))
            .unwrap_or_default()
    }

    /// Where the comment being written starts, when it takes in lines above.
    pub(super) fn comment_start(&self, path: &str, line: u32, side: &str) -> Option<u32> {
        range(&self.rows_of(path), line, side, self.review.span).0
    }

    pub(crate) fn reach(
        &mut self,
        path: &str,
        line: u32,
        side: &str,
        more: bool,
        cx: &mut Context<Self>,
    ) {
        let most = max_span(&self.rows_of(path), line, side);
        self.review.span = if more {
            (self.review.span + 1).min(most)
        } else {
            self.review.span.saturating_sub(1)
        };
        cx.notify();
    }

    /// Puts the suggestion block for the lines covered at the end of what
    /// is written.
    pub(crate) fn suggest(&mut self, path: &str, line: u32, side: &str, cx: &mut Context<Self>) {
        let (_, lines) = range(&self.rows_of(path), line, side, self.review.span);
        if lines.is_empty() {
            return;
        }
        let draft = self.review.draft.clone();
        draft.update(cx, |area, cx| {
            let written = area.text().trim_end().to_string();
            let block = suggestion(&lines);
            let text = if written.is_empty() {
                block
            } else {
                format!("{written}\n\n{block}")
            };
            area.set_text(&text, cx);
        });
        cx.notify();
    }

    /// In the line editor: the lines covered, their "−"/"+", and "Sugerir"
    /// (on the new side only: a suggestion replaces what the pull request
    /// has there).
    pub(super) fn range_row(
        &self,
        path: &str,
        line: u32,
        side: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let rows = self.rows_of(path);
        let most = max_span(&rows, line, side);
        let span = self.review.span.min(most);
        let start = range(&rows, line, side, span).0.unwrap_or(line);
        let label = if start == line {
            cx.t_with(
                "pullRequests.code.range.one",
                &Args::default().with("line", line.to_string()),
            )
        } else {
            cx.t_with(
                "pullRequests.code.lines",
                &Args::default()
                    .with("from", start.to_string())
                    .with("to", line.to_string()),
            )
        };
        let step = |id: &'static str,
                    icon: &'static str,
                    more: bool,
                    off: bool,
                    cx: &mut Context<Self>| {
            let path = path.to_string();
            Button::new(id)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconXs)
                .disabled(off)
                .on_click(cx.listener(move |hub, _, _, cx| hub.reach(&path, line, side, more, cx)))
                .child(Icon::new(icon).size(12.0))
        };
        let suggested = path.to_string();
        div()
            .debug_selector(|| "pr-line-range".into())
            .mt(px(8.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(11.0))
            .text_color(colors.muted_foreground)
            .child(div().flex_1().min_w_0().truncate().child(label))
            .child(step("pr-line-range-less", "minus", false, span == 0, cx))
            .child(step("pr-line-range-more", "plus", true, span >= most, cx))
            .when(side == "right", |row| {
                row.child(
                    div().debug_selector(|| "pr-line-suggest".into()).child(
                        Button::new("pr-line-suggest")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Xs)
                            .text_size(11.0)
                            .on_click(cx.listener(move |hub, _, _, cx| {
                                hub.suggest(&suggested, line, side, cx)
                            }))
                            .child(cx.t("pullRequests.code.range.suggest")),
                    ),
                )
            })
            .into_any_element()
    }
}
