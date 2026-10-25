//! Pull request conversations on the diff (`machineAnnotations` with
//! `source: "forge-review"`): the commented lines get a 3px bar on their
//! side's color (`workspaceDiffAnnotationCss`), and under the last one a
//! callout with "@author: first line" (`AnnotationCallout`) that opens the
//! conversation. A line can be brought to the middle of the view.

use dcc_core::domain::code_annotation::{AnnotationSide, DiffColumn};
use dcc_core::domain::review_thread_detail::ReviewMark;
use gpui::{AnyElement, Context, ListOffset, SharedString, div, prelude::*, px};

use super::{DiffView, DiffViewEvent, State};
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The marks' own cap (`line < start + 500`): a bad finding draws no more.
const MAX_MARKED_LINES: usize = 500;

fn column_of(side: AnnotationSide) -> DiffColumn {
    match side {
        AnnotationSide::Original => DiffColumn::Deletions,
        AnnotationSide::Modified => DiffColumn::Additions,
    }
}

impl DiffView {
    /// The conversations to mark, replacing the ones before.
    pub fn set_marks(&mut self, marks: Vec<ReviewMark>, cx: &mut Context<Self>) {
        if self.marks != marks {
            self.marks = marks;
            self.list.remeasure();
            cx.notify();
        }
    }

    /// The line sits in a marked range on its side.
    pub(crate) fn marked(&self, column: DiffColumn, line: usize) -> bool {
        self.marks.iter().any(|mark| {
            column_of(mark.side) == column
                && (mark.start_line..=mark.end_line.min(mark.start_line + MAX_MARKED_LINES))
                    .contains(&line)
        })
    }

    /// The callouts under the last line of each mark ending here.
    pub(crate) fn callouts(
        &self,
        column: DiffColumn,
        line: usize,
        left: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ending: Vec<&ReviewMark> = self
            .marks
            .iter()
            .filter(|mark| column_of(mark.side) == column && mark.end_line.max(1) == line)
            .collect();
        if ending.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let fonts = cx.theme().fonts.clone();
        let hover = colors.muted;
        Some(
            div()
                .flex()
                .flex_col()
                .items_start()
                .pl(px(left + 8.0))
                .pr(px(8.0))
                .py(px(2.0))
                .bg(self.palette().background)
                .font_family(fonts.sans.clone())
                .children(ending.into_iter().map(|mark| {
                    let id = mark.id.clone();
                    let selector = format!("review-mark-{id}");
                    div()
                        .id(SharedString::from(selector.clone()))
                        .debug_selector(move || selector.clone())
                        .my(px(4.0))
                        .flex()
                        .max_w_full()
                        .items_center()
                        .gap(px(6.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.7))
                        .bg(colors.muted.opacity(0.7))
                        .px(px(8.0))
                        .py(px(4.0))
                        .text_size(px(10.0))
                        .line_height(px(15.0))
                        .text_color(colors.foreground)
                        .shadow_sm()
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(DiffViewEvent::OpenMark(id.clone()))
                        }))
                        .child(Icon::new("message-square").size(12.0))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(SharedString::from(mark.title.clone())),
                        )
                }))
                .into_any_element(),
        )
    }

    /// The row showing `line` on `side`.
    fn row_of(&self, side: AnnotationSide, line: usize) -> Option<usize> {
        let column = column_of(side);
        if self.split {
            (0..self.split_rows.len()).find(|row| {
                self.split_point(*row, column)
                    .is_some_and(|point| point.line == line)
            })
        } else {
            (0..self.rows.len()).find(|row| {
                self.unified_point(*row)
                    .is_some_and(|point| point.column == column && point.line == line)
            })
        }
    }

    /// `scrollTo({ type: "line", align: "center" })`, once the lines are in.
    pub fn reveal_line(&mut self, side: AnnotationSide, line: usize, cx: &mut Context<Self>) {
        if !matches!(self.state, State::Ready(_)) {
            self.pending_reveal = Some((side, line));
            return;
        }
        // A folded line opens first.
        if self.row_of(side, line).is_none() {
            self.open_line(side, line, cx);
        }
        let Some(row) = self.row_of(side, line) else {
            return;
        };
        let height = f32::from(self.list.viewport_bounds().size.height);
        let above = ((height / 2.0) / 20.0).floor() as usize;
        self.list.scroll_to(ListOffset {
            item_ix: row.saturating_sub(above),
            offset_in_item: px(0.0),
        });
        cx.notify();
    }

    /// Opens the folded runs until `line` shows.
    fn open_line(&mut self, side: AnnotationSide, line: usize, cx: &mut Context<Self>) {
        let State::Ready(computed) = &self.state else {
            return;
        };
        let gap = computed
            .diff
            .gap_holding(side == AnnotationSide::Original, line);
        if let Some(gap) = gap {
            self.expand(gap, true, cx);
        }
    }

    pub(crate) fn take_pending_reveal(&mut self, cx: &mut Context<Self>) {
        if let Some((side, line)) = self.pending_reveal.take() {
            self.reveal_line(side, line, cx);
        }
    }
}
