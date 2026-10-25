//! Picking lines to send to the agent (`enableLineSelection` and
//! `enableGutterUtility` in the Pierre diff, as the DCC turns them on):
//! a press on a line number picks it, a drag goes on to other lines, Shift
//! extends, and pressing the only picked line again drops it. The "+" on
//! the line under the pointer (on the pick's last line while there is
//! one) is pressed and dragged over the lines, and letting go sends them;
//! "Enviar ao agente" over the diff sends the pick as it is.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use dcc_core::domain::code_annotation::{DiffColumn, DiffLineRange, from_diff_range};
use gpui::{Bounds, Context, Pixels, Point, point};

use super::DiffView;
use crate::data::diff::{LineKind, Row, SplitRow};
use crate::screens::annotation::AnnotationRequest;

/// A line of one column, at its row in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinePoint {
    pub row: usize,
    pub column: DiffColumn,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinePick {
    pub anchor: LinePoint,
    pub head: LinePoint,
}

impl LinePick {
    /// The rows it covers, top first.
    pub fn rows(&self) -> std::ops::RangeInclusive<usize> {
        self.anchor.row.min(self.head.row)..=self.anchor.row.max(self.head.row)
    }

    fn ends(&self) -> (LinePoint, LinePoint) {
        if self.anchor.row <= self.head.row {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The last line, where the "+" stays.
    pub fn bottom(&self) -> LinePoint {
        self.ends().1
    }

    pub fn covers(&self, row: usize, column: DiffColumn, split: bool) -> bool {
        self.rows().contains(&row)
            && (!split || column == self.anchor.column || column == self.head.column)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    /// From a line number.
    Selecting,
    /// From the "+": letting go sends.
    Gutter,
    /// The only picked line pressed again: dropped unless the pointer moves.
    Unselect(LinePoint),
}

/// What the diff knows to send a pick: the file and both whole texts.
pub struct Source {
    pub path: String,
    pub original: Arc<str>,
    pub modified: Arc<str>,
}

impl Source {
    pub fn new(path: &str, original: &str, modified: &str) -> Self {
        Self {
            path: path.to_string(),
            original: Arc::from(original),
            modified: Arc::from(modified),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiffViewEvent {
    /// Lines to send to the agent.
    Annotate(AnnotationRequest),
    /// A pull request conversation's callout, by thread id.
    OpenMark(String),
}

/// The pick state of an annotating diff.
pub struct Picking {
    pub source: Source,
    pub pick: Option<LinePick>,
    pub drag: Option<Drag>,
    /// The diff's bounds and the pill's, from the last frame.
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    pub pill: Rc<Cell<Bounds<Pixels>>>,
}

impl Picking {
    pub fn new(source: Source) -> Self {
        Self {
            source,
            pick: None,
            drag: None,
            bounds: Rc::default(),
            pill: Rc::default(),
        }
    }
}

impl DiffView {
    /// The line a unified row shows: removed lines by their old number.
    pub(crate) fn unified_point(&self, row: usize) -> Option<LinePoint> {
        let super::State::Ready(computed) = &self.state else {
            return None;
        };
        let Some(Row::Line(index)) = self.rows.get(row) else {
            return None;
        };
        let line = &computed.diff.lines[*index];
        match line.kind {
            LineKind::Deletion => line.old.map(|number| LinePoint {
                row,
                column: DiffColumn::Deletions,
                line: number,
            }),
            _ => line.new.map(|number| LinePoint {
                row,
                column: DiffColumn::Additions,
                line: number,
            }),
        }
    }

    /// The line one column of a side-by-side row shows.
    pub(crate) fn split_point(&self, row: usize, column: DiffColumn) -> Option<LinePoint> {
        let super::State::Ready(computed) = &self.state else {
            return None;
        };
        let Some(SplitRow::Pair { old, new }) = self.split_rows.get(row) else {
            return None;
        };
        let line = match column {
            DiffColumn::Deletions => computed.diff.lines[(*old)?].old,
            DiffColumn::Additions => computed.diff.lines[(*new)?].new,
        }?;
        Some(LinePoint { row, column, line })
    }

    pub(crate) fn picking(&self) -> bool {
        self.picking.is_some()
    }

    /// A press on a line number.
    pub(crate) fn press_number(&mut self, at: LinePoint, shift: bool, cx: &mut Context<Self>) {
        let Some(picking) = self.picking.as_mut() else {
            return;
        };
        match picking.pick {
            Some(pick) if shift => {
                picking.pick = Some(LinePick { head: at, ..pick });
                picking.drag = Some(Drag::Selecting);
            }
            Some(pick) if pick.anchor == at && pick.head == at => {
                picking.drag = Some(Drag::Unselect(at));
            }
            _ => {
                picking.pick = Some(LinePick {
                    anchor: at,
                    head: at,
                });
                picking.drag = Some(Drag::Selecting);
            }
        }
        cx.notify();
    }

    /// A press on the "+": from the pick's top (or this line) on.
    pub(crate) fn press_plus(&mut self, at: LinePoint, cx: &mut Context<Self>) {
        let Some(picking) = self.picking.as_mut() else {
            return;
        };
        let pick = match picking.pick {
            Some(pick) => {
                let (top, bottom) = pick.ends();
                LinePick {
                    anchor: top,
                    head: bottom,
                }
            }
            None => LinePick {
                anchor: at,
                head: at,
            },
        };
        picking.pick = Some(pick);
        picking.drag = Some(Drag::Gutter);
        cx.notify();
    }

    /// The pointer over a line while a press lasts.
    pub(crate) fn drag_over(&mut self, at: LinePoint, cx: &mut Context<Self>) {
        let Some(picking) = self.picking.as_mut() else {
            return;
        };
        match picking.drag {
            Some(Drag::Selecting | Drag::Gutter) => {
                if let Some(pick) = picking.pick.as_mut()
                    && pick.head != at
                {
                    pick.head = at;
                    cx.notify();
                }
            }
            Some(Drag::Unselect(pressed)) if pressed != at => {
                picking.pick = Some(LinePick {
                    anchor: pressed,
                    head: at,
                });
                picking.drag = Some(Drag::Selecting);
                cx.notify();
            }
            _ => {}
        }
    }

    /// The press ends anywhere.
    pub(crate) fn release(&mut self, cx: &mut Context<Self>) {
        let Some(picking) = self.picking.as_mut() else {
            return;
        };
        match picking.drag.take() {
            Some(Drag::Unselect(_)) => {
                picking.pick = None;
                cx.notify();
            }
            Some(Drag::Gutter) => {
                let bounds = picking.bounds.get();
                self.annotate(point(bounds.left(), bounds.bottom()), cx);
            }
            _ => {}
        }
    }

    /// The pick goes to the agent; the diff lets go of it.
    pub(crate) fn annotate(&mut self, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(picking) = self.picking.as_mut() else {
            return;
        };
        let Some(pick) = picking.pick.take() else {
            return;
        };
        picking.drag = None;
        let range = DiffLineRange {
            start: pick.anchor.line,
            end: pick.head.line,
            side: pick.anchor.column,
            end_side: pick.head.column,
        };
        let source = &picking.source;
        let annotation = from_diff_range(&source.path, range, &source.original, &source.modified);
        cx.emit(DiffViewEvent::Annotate(AnnotationRequest {
            annotation,
            anchor,
        }));
        cx.notify();
    }
}
