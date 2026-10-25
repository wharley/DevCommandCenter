//! `ScrollableTabStrip`: the stage's tab row. No scrollbar: when tabs
//! overflow, the hidden side fades out and shows an arrow that moves most of
//! a strip, the mouse wheel scrolls sideways, and the selected tab is kept
//! in view clear of the arrows.

use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, ScrollDelta,
    ScrollHandle, SharedString, Styled, Window, div, linear_color_stop, linear_gradient, point,
    prelude::*, px,
};
use smallvec::SmallVec;

use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// `TAB_STRIP_EDGE_INSET`: the faded edge an arrow sits on.
pub const EDGE_INSET: f32 = 40.0;

/// `tabStripOverflow`: the sides that still hide tabs (sub-pixel rounding
/// ignored). `offset` is the scroll offset (zero or negative).
pub fn overflow(offset: f32, max_offset: f32) -> (bool, bool) {
    (offset < -1.0, -offset < max_offset - 1.0)
}

/// `scrollDeltaToReveal`: how far to scroll so the tab is fully in view,
/// clear of the edges; zero when it already is.
pub fn delta_to_reveal(tab_start: f32, tab_end: f32, view_start: f32, view_width: f32) -> f32 {
    let visible_start = view_start + EDGE_INSET;
    let visible_end = view_start + view_width - EDGE_INSET;
    if tab_end - tab_start > visible_end - visible_start || tab_start < visible_start {
        tab_start - visible_start
    } else if tab_end > visible_end {
        tab_end - visible_end
    } else {
        0.0
    }
}

/// `arrowScrollStep`: one click moves most of a strip, keeping a tab of context.
pub fn arrow_step(width: f32) -> f32 {
    (width * 0.7).round().max(80.0)
}

/// The strip's scroll, kept by its owner across renders.
#[derive(Default, Clone)]
pub struct TabStripState {
    pub handle: ScrollHandle,
    /// The selected tab to bring into view, until it is.
    reveal: Option<(usize, SharedString)>,
    shown: Option<SharedString>,
}

impl TabStripState {
    /// Keeps the tab at `index` in view whenever `key` changes.
    pub fn reveal(&mut self, index: usize, key: impl Into<SharedString>) {
        let key = key.into();
        if self.shown.as_ref() != Some(&key) {
            self.reveal = Some((index, key));
        }
    }

    fn scroll_to(&self, x: f32) {
        let max = f32::from(self.handle.max_offset().x);
        self.handle
            .set_offset(point(px(x.clamp(-max, 0.0)), px(0.0)));
    }

    /// The reveal waiting for the tab's bounds (known after a layout).
    fn settle(&mut self) {
        let Some((index, key)) = self.reveal.clone() else {
            return;
        };
        let Some(tab) = self.handle.bounds_for_item(index) else {
            return;
        };
        let view = self.handle.bounds();
        let offset = f32::from(self.handle.offset().x);
        // The tab's place in the scrolled content.
        let start = f32::from(tab.left() - view.left()) - offset;
        let end = start + f32::from(tab.size.width);
        let delta = delta_to_reveal(start, end, -offset, f32::from(view.size.width));
        if delta != 0.0 {
            self.scroll_to(offset - delta);
        }
        self.reveal = None;
        self.shown = Some(key);
    }
}

#[derive(IntoElement)]
pub struct TabStrip {
    id: ElementId,
    state: TabStripState,
    tabs: SmallVec<[AnyElement; 4]>,
}

impl TabStrip {
    /// `state` is a clone of the owner's (the scroll handle is shared).
    pub fn new(id: impl Into<ElementId>, state: &mut TabStripState) -> Self {
        state.settle();
        Self {
            id: id.into(),
            state: state.clone(),
            tabs: SmallVec::new(),
        }
    }
}

impl ParentElement for TabStrip {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.tabs.extend(elements);
    }
}

fn edge_arrow(id: ElementId, left: bool, state: TabStripState, cx: &App) -> AnyElement {
    let colors = cx.theme().colors.clone();
    let hover = colors.muted.opacity(0.6);
    let foreground = colors.foreground;
    let fade = linear_gradient(
        if left { 90.0 } else { 270.0 },
        linear_color_stop(colors.background, 0.0),
        linear_color_stop(colors.background.opacity(0.0), 1.0),
    );
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(EDGE_INSET))
        .map(|edge| if left { edge.left_0() } else { edge.right_0() })
        .bg(fade)
        .child(
            div()
                .id(id)
                .absolute()
                .top(gpui::relative(0.5))
                .mt(px(-12.0))
                .map(|arrow| {
                    if left {
                        arrow.left(px(2.0))
                    } else {
                        arrow.right(px(2.0))
                    }
                })
                .flex()
                .size(px(24.0))
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .bg(colors.background)
                .text_color(colors.muted_foreground)
                .cursor_pointer()
                .hover(move |style| style.bg(hover).text_color(foreground))
                .on_click(move |_, window, _| {
                    let width = f32::from(state.handle.bounds().size.width);
                    let offset = f32::from(state.handle.offset().x);
                    let step = arrow_step(width);
                    state.scroll_to(if left { offset + step } else { offset - step });
                    window.refresh();
                })
                .child(
                    Icon::new(if left {
                        "chevron-left"
                    } else {
                        "chevron-right"
                    })
                    .size(14.0),
                ),
        )
        .into_any_element()
}

impl RenderOnce for TabStrip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let handle = self.state.handle.clone();
        let (left, right) = overflow(
            f32::from(handle.offset().x),
            f32::from(handle.max_offset().x),
        );
        let wheel = handle.clone();
        let scroller = div()
            .id(self.id.clone())
            .flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .items_center()
            .gap(px(2.0))
            .px(px(8.0))
            .overflow_x_scroll()
            .track_scroll(&handle)
            // A vertical wheel scrolls sideways.
            .on_scroll_wheel(move |event, window, _| {
                let delta: Pixels = match event.delta {
                    ScrollDelta::Pixels(delta) if delta.y.abs() > delta.x.abs() => delta.y,
                    ScrollDelta::Lines(delta) if delta.y.abs() > delta.x.abs() => {
                        px(delta.y * 16.0)
                    }
                    _ => return,
                };
                let max = wheel.max_offset().x;
                if max <= px(0.0) {
                    return;
                }
                let x = (wheel.offset().x + delta).clamp(-max, px(0.0));
                wheel.set_offset(point(x, px(0.0)));
                window.refresh();
            })
            .children(self.tabs);
        let id = self.id;
        div()
            .relative()
            .flex()
            .flex_1()
            .h_full()
            .min_w_0()
            .items_center()
            .child(scroller)
            .when(left, |strip| {
                strip.child(edge_arrow(
                    ElementId::Name(format!("{id:?}-left").into()),
                    true,
                    self.state.clone(),
                    cx,
                ))
            })
            .when(right, |strip| {
                strip.child(edge_arrow(
                    ElementId::Name(format!("{id:?}-right").into()),
                    false,
                    self.state.clone(),
                    cx,
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_knows_which_side_hides_tabs() {
        assert_eq!(overflow(0.0, 0.0), (false, false));
        assert_eq!(overflow(0.0, 120.0), (false, true));
        assert_eq!(overflow(-60.0, 120.0), (true, true));
        assert_eq!(overflow(-120.0, 120.0), (true, false));
    }

    #[test]
    fn a_selected_tab_is_scrolled_clear_of_the_arrows() {
        // In view: nothing to do.
        assert_eq!(delta_to_reveal(100.0, 160.0, 0.0, 400.0), 0.0);
        // Past the right edge: just enough to clear the inset.
        assert_eq!(delta_to_reveal(380.0, 440.0, 0.0, 400.0), 80.0);
        // Before the left edge: back to the inset.
        assert_eq!(delta_to_reveal(10.0, 60.0, 100.0, 400.0), -130.0);
        assert_eq!(arrow_step(100.0), 80.0);
        assert_eq!(arrow_step(400.0), 280.0);
    }
}
