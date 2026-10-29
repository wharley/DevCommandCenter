//! Selecting rendered text, like the desktop's timeline (a web page): each
//! piece of prose a message draws registers its laid-out text every frame,
//! in document order, and the owner (the conversation) turns the mouse into
//! a range kept as byte anchors. A selection stays inside one message (its
//! `scope`); the highlight is painted behind the text. Built after
//! MonoCode's transcript selection.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, SharedString, TextLayout, Window, fill, point,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::theme::ActiveTheme;

/// A spot in a message: the piece of prose (in document order) and a byte.
type Spot = (usize, usize);

struct Fragment {
    scope: SharedString,
    index: usize,
    text: SharedString,
    layout: TextLayout,
    bounds: Bounds<Pixels>,
}

#[derive(Default)]
struct State {
    /// This frame's prose, as painted.
    fragments: Vec<Fragment>,
    scope: Option<SharedString>,
    anchor: Spot,
    head: Spot,
    /// The mouse went down on text and has not come up.
    dragging: bool,
    /// What a double or triple click grabbed, kept while dragging on.
    grabbed: Option<(Spot, Spot)>,
}

#[derive(Clone, Default)]
pub struct Selection(Rc<RefCell<State>>);

impl Selection {
    /// Forgets last frame's prose; the frame about to draw registers again.
    pub fn begin_frame(&self) {
        self.0.borrow_mut().fragments.clear();
    }

    pub fn is_empty(&self) -> bool {
        let state = self.0.borrow();
        state.scope.is_none() || state.anchor == state.head
    }

    pub fn dragging(&self) -> bool {
        self.0.borrow().dragging
    }

    /// Drops the selection; true when there was one.
    pub fn clear(&self) -> bool {
        let mut state = self.0.borrow_mut();
        let had = state.scope.is_some();
        state.scope = None;
        state.dragging = false;
        state.grabbed = None;
        had
    }

    /// The mouse goes down: on prose it starts a selection (a word on a
    /// double click, the whole piece on a triple) and returns true.
    pub fn press(&self, position: Point<Pixels>, clicks: usize) -> bool {
        let mut state = self.0.borrow_mut();
        let Some(fragment) = state
            .fragments
            .iter()
            .find(|fragment| fragment.bounds.contains(&position))
        else {
            return false;
        };
        let byte = closest(&fragment.layout, position);
        let (start, end) = match clicks {
            2 => word_at(&fragment.text, byte),
            n if n >= 3 => (0, fragment.text.len()),
            _ => (byte, byte),
        };
        let (scope, index) = (fragment.scope.clone(), fragment.index);
        state.scope = Some(scope);
        state.anchor = (index, start);
        state.head = (index, end);
        state.grabbed = (clicks >= 2).then_some(((index, start), (index, end)));
        state.dragging = true;
        true
    }

    /// The mouse moves with the button down: the head follows it inside the
    /// message. True when the range changed.
    pub fn drag(&self, position: Point<Pixels>) -> bool {
        let mut state = self.0.borrow_mut();
        if !state.dragging {
            return false;
        }
        let Some(scope) = state.scope.clone() else {
            return false;
        };
        let Some(spot) = spot_near(&state.fragments, &scope, position) else {
            return false;
        };
        let (anchor, head) = match state.grabbed {
            // Dragging on from a word keeps the whole word.
            Some((start, end)) if spot < start => (end, spot),
            Some((start, end)) => (start, spot.max(end)),
            None => (state.anchor, spot),
        };
        let changed = (anchor, head) != (state.anchor, state.head);
        state.anchor = anchor;
        state.head = head;
        changed
    }

    pub fn release(&self) {
        let mut state = self.0.borrow_mut();
        state.dragging = false;
        if state.anchor == state.head {
            state.scope = None;
        }
    }

    /// Whether `position` is on selected text (a right click keeps it).
    pub fn covers(&self, position: Point<Pixels>) -> bool {
        let state = self.0.borrow();
        state.fragments.iter().any(|fragment| {
            fragment.bounds.contains(&position)
                && !range_in(&state, &fragment.scope, fragment.index, fragment.text.len())
                    .is_empty()
        })
    }

    /// The selected text, piece by piece on their own lines.
    pub fn text(&self) -> Option<String> {
        let state = self.0.borrow();
        let scope = state.scope.as_ref()?;
        let mut pieces: Vec<&Fragment> = state
            .fragments
            .iter()
            .filter(|fragment| &fragment.scope == scope)
            .collect();
        pieces.sort_by_key(|fragment| fragment.index);
        pieces.dedup_by_key(|fragment| fragment.index);
        let text: Vec<&str> = pieces
            .into_iter()
            .filter_map(|fragment| {
                let range = range_in(&state, scope, fragment.index, fragment.text.len());
                (!range.is_empty()).then(|| &fragment.text[range])
            })
            .collect();
        (!text.is_empty()).then(|| text.join("\n"))
    }

    fn register(&self, fragment: Fragment) {
        self.0.borrow_mut().fragments.push(fragment);
    }

    fn range(&self, scope: &SharedString, index: usize, len: usize) -> Range<usize> {
        range_in(&self.0.borrow(), scope, index, len)
    }
}

/// The part of piece `index` (of `len` bytes) the selection covers.
fn range_in(state: &State, scope: &SharedString, index: usize, len: usize) -> Range<usize> {
    if state.scope.as_ref() != Some(scope) || state.anchor == state.head {
        return 0..0;
    }
    let (start, end) = if state.anchor <= state.head {
        (state.anchor, state.head)
    } else {
        (state.head, state.anchor)
    };
    if index < start.0 || index > end.0 {
        return 0..0;
    }
    let from = if index == start.0 {
        start.1.min(len)
    } else {
        0
    };
    let to = if index == end.0 { end.1.min(len) } else { len };
    from.min(to)..to
}

fn closest(layout: &TextLayout, position: Point<Pixels>) -> usize {
    match layout.index_for_position(position) {
        Ok(index) | Err(index) => index,
    }
}

/// Where the mouse is in the message: on a piece, or the nearest edge of
/// the piece above it (past its end) or the first one (its start).
fn spot_near(
    fragments: &[Fragment],
    scope: &SharedString,
    position: Point<Pixels>,
) -> Option<Spot> {
    let mut pieces: Vec<&Fragment> = fragments
        .iter()
        .filter(|fragment| &fragment.scope == scope)
        .collect();
    pieces.sort_by_key(|fragment| fragment.index);
    if let Some(fragment) = pieces
        .iter()
        .find(|fragment| fragment.bounds.contains(&position))
    {
        return Some((fragment.index, closest(&fragment.layout, position)));
    }
    let above = pieces
        .iter()
        .rev()
        .find(|fragment| fragment.bounds.top() <= position.y);
    match above {
        Some(fragment) if position.y > fragment.bounds.bottom() => {
            Some((fragment.index, fragment.text.len()))
        }
        Some(fragment) => {
            let y = position.y.min(fragment.bounds.bottom() - gpui::px(1.0));
            Some((
                fragment.index,
                closest(&fragment.layout, point(position.x, y)),
            ))
        }
        None => pieces.first().map(|fragment| (fragment.index, 0)),
    }
}

/// The word (or the run of spaces or punctuation) around `byte`.
fn word_at(text: &str, byte: usize) -> (usize, usize) {
    text.split_word_bound_indices()
        .map(|(start, word)| (start, start + word.len()))
        .find(|&(start, end)| start <= byte && byte < end)
        .unwrap_or((byte, byte))
}

/// While a selectable message is being built: its selection, its scope and
/// the next piece's index.
struct Building {
    selection: Selection,
    scope: SharedString,
    next: usize,
}

thread_local! {
    /// The selection the view being drawn offers its messages.
    static ACTIVE: RefCell<Option<Selection>> = const { RefCell::new(None) };
    static BUILDING: RefCell<Option<Building>> = const { RefCell::new(None) };
}

/// Draws with `selection` offered: the messages built inside can be selected.
pub fn within<R>(selection: &Selection, draw: impl FnOnce() -> R) -> R {
    let previous = ACTIVE.with(|active| active.borrow_mut().replace(selection.clone()));
    let drawn = draw();
    ACTIVE.with(|active| *active.borrow_mut() = previous);
    drawn
}

/// Builds message `scope`; its prose is selectable when a selection is
/// offered (`within`).
pub(super) fn message<R>(scope: &str, build: impl FnOnce() -> R) -> R {
    let Some(selection) = ACTIVE.with(|active| active.borrow().clone()) else {
        return build();
    };
    let previous = BUILDING.with(|building| {
        building.borrow_mut().replace(Building {
            selection,
            scope: SharedString::from(scope.to_string()),
            next: 0,
        })
    });
    let built = build();
    BUILDING.with(|building| *building.borrow_mut() = previous);
    built
}

/// Wraps a piece of prose so it can be selected, when a selectable message
/// is being built; otherwise leaves it as is.
pub(super) fn wrap(text: SharedString, layout: TextLayout, child: AnyElement) -> AnyElement {
    let piece = BUILDING.with(|building| {
        building.borrow_mut().as_mut().map(|building| {
            let index = building.next;
            building.next += 1;
            (building.selection.clone(), building.scope.clone(), index)
        })
    });
    match piece {
        Some((selection, scope, index)) => Selectable {
            selection,
            scope,
            index,
            text,
            layout,
            child: Some(child),
        }
        .into_any_element(),
        None => child,
    }
}

struct Selectable {
    selection: Selection,
    scope: SharedString,
    index: usize,
    text: SharedString,
    layout: TextLayout,
    child: Option<AnyElement>,
}

impl IntoElement for Selectable {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Selectable {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut child = self.child.take().expect("laid out once");
        (child.request_layout(window, cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        child.prepaint(window, cx);
        self.selection.register(Fragment {
            scope: self.scope.clone(),
            index: self.index,
            text: self.text.clone(),
            layout: self.layout.clone(),
            bounds,
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let range = self
            .selection
            .range(&self.scope, self.index, self.text.len());
        if !range.is_empty() {
            // `::selection`: the accent at 26%, like the fields.
            let color = cx.theme().colors.sidebar_primary.opacity(0.26);
            for quad in highlight(&self.layout, bounds, range) {
                window.paint_quad(fill(quad, color));
            }
        }
        child.paint(window, cx);
    }
}

/// The rectangles behind `range`: from the start to the right edge, whole
/// lines between, the left edge to the end.
fn highlight(
    layout: &TextLayout,
    bounds: Bounds<Pixels>,
    range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let (Some(start), Some(end)) = (
        layout.position_for_index(range.start),
        layout.position_for_index(range.end),
    ) else {
        return Vec::new();
    };
    let line = layout.line_height();
    let span = |left: Pixels, top: Pixels, right: Pixels, bottom: Pixels| {
        Bounds::from_corners(point(left, top), point(right, bottom))
    };
    if start.y == end.y {
        return vec![span(start.x, start.y, end.x, start.y + line)];
    }
    let mut quads = vec![span(start.x, start.y, bounds.right(), start.y + line)];
    if end.y > start.y + line {
        quads.push(span(bounds.left(), start.y + line, bounds.right(), end.y));
    }
    quads.push(span(bounds.left(), end.y, end.x, end.y + line));
    quads
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Render, TestAppContext, VisualTestContext, WindowAppearance, div, prelude::*};

    use crate::markdown::{MarkdownStyle, parse, render_markdown};
    use crate::theme::{Theme, ThemePreference};

    struct Thread(Selection, Vec<crate::markdown::Block>);

    impl Render for Thread {
        fn render(
            &mut self,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            self.0.begin_frame();
            let element = within(&self.0, || {
                render_markdown(&self.1, MarkdownStyle::reply(), "m1", window, cx)
            });
            div()
                .debug_selector(|| "thread".into())
                .w(gpui::px(600.0))
                .child(element)
        }
    }

    #[gpui::test]
    fn dragging_over_a_message_selects_its_prose(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
            crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        });
        let selection = Selection::default();
        let blocks = parse("Rode os testes agora.\n\n- primeiro item\n- segundo item\n");
        let shared = selection.clone();
        let (_, cx): (_, &mut VisualTestContext) =
            cx.add_window_view(move |_, _| Thread(shared.clone(), blocks.clone()));
        cx.run_until_parked();
        let bounds = cx.debug_bounds("thread").expect("drawn");
        // From the very first letter to below the last item.
        assert!(selection.press(bounds.origin + point(gpui::px(1.0), gpui::px(4.0)), 1));
        assert!(selection.drag(point(bounds.right(), bounds.bottom() + gpui::px(40.0))));
        selection.release();
        assert_eq!(
            selection.text().as_deref(),
            Some("Rode os testes agora.\nprimeiro item\nsegundo item")
        );
        // A double click on "Rode" takes the word.
        assert!(selection.press(bounds.origin + point(gpui::px(6.0), gpui::px(4.0)), 2));
        selection.release();
        assert_eq!(selection.text().as_deref(), Some("Rode"));
        // A press away from the prose leaves nothing to copy.
        assert!(!selection.press(point(bounds.right() + gpui::px(50.0), bounds.top()), 1));
        selection.clear();
        assert!(selection.text().is_none());
    }

    fn state(anchor: Spot, head: Spot) -> State {
        State {
            scope: Some("m".into()),
            anchor,
            head,
            ..State::default()
        }
    }

    #[test]
    fn a_range_spans_pieces_in_document_order() {
        let scope = SharedString::from("m");
        // Backwards drag: from piece 2 byte 3 up to piece 0 byte 4.
        let state = state((2, 3), (0, 4));
        assert_eq!(range_in(&state, &scope, 0, 10), 4..10);
        assert_eq!(range_in(&state, &scope, 1, 7), 0..7);
        assert_eq!(range_in(&state, &scope, 2, 9), 0..3);
        assert_eq!(range_in(&state, &scope, 3, 9), 0..0);
        assert_eq!(range_in(&state, &"other".into(), 1, 7), 0..0);
    }

    #[test]
    fn a_double_click_takes_the_word() {
        assert_eq!(word_at("rode os testes", 9), (8, 14));
        assert_eq!(word_at("rode os testes", 0), (0, 4));
    }
}
