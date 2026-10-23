//! A thin overlay scrollbar for a virtual `list` (`scrollbar-width: thin` on
//! macOS): it shows while the list scrolls or the thumb is held, fades a
//! second later, and the thumb drags. The owner tells it when the list last
//! moved; the geometry comes from the list's last layout.

use std::time::{Duration, Instant};

use gpui::{
    App, AppContext, DragMoveEvent, ElementId, Empty, ListState, Window, div, point, prelude::*, px,
};

use crate::theme::{ActiveTheme, Appearance};

/// How long the bar stays after the list stops.
pub const SCROLLBAR_LINGER: Duration = Duration::from_millis(1000);
const WIDTH: f32 = 7.0;
const INSET: f32 = 2.0;
const MIN_THUMB: f32 = 24.0;

/// Drag payload of the thumb.
#[derive(Clone, Copy)]
struct ThumbDrag;

#[derive(IntoElement)]
pub struct ListScrollbar {
    id: ElementId,
    state: ListState,
    visible: bool,
}

impl ListScrollbar {
    pub fn new(id: impl Into<ElementId>, state: ListState, last_scrolled: Option<Instant>) -> Self {
        Self {
            id: id.into(),
            visible: last_scrolled.is_some_and(|at| at.elapsed() < SCROLLBAR_LINGER)
                || state.is_scrollbar_dragging(),
            state,
        }
    }
}

/// Thumb top and height for a viewport over `max_offset` of extra content.
pub fn thumb_geometry(viewport: f32, max_offset: f32, scrolled: f32) -> Option<(f32, f32)> {
    if viewport <= 0.0 || max_offset <= 0.5 {
        return None;
    }
    let track = viewport - INSET * 2.0;
    let height = (track * viewport / (viewport + max_offset))
        .max(MIN_THUMB)
        .min(track);
    let progress = (scrolled / max_offset).clamp(0.0, 1.0);
    Some((INSET + (track - height) * progress, height))
}

impl RenderOnce for ListScrollbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Where on the thumb the pointer grabbed it, kept across frames.
        let grab = window.use_keyed_state(self.id.clone(), cx, |_, _| px(0.0));
        let grab_read = grab.clone();
        let bounds = self.state.viewport_bounds();
        let viewport = f32::from(bounds.size.height);
        let max_offset = f32::from(self.state.max_offset_for_scrollbar().y);
        let scrolled = -f32::from(self.state.scroll_px_offset_for_scrollbar().y);
        let Some((top, height)) = thumb_geometry(viewport, max_offset, scrolled) else {
            return div().into_any_element();
        };
        let color = match cx.theme().appearance {
            Appearance::Light => gpui::black().opacity(0.45),
            Appearance::Dark => gpui::white().opacity(0.45),
        };
        let drag_state = self.state.clone();
        let end_state = self.state.clone();
        let start_state = self.state.clone();
        let track_top = bounds.origin.y;
        div()
            .id(self.id)
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(WIDTH + INSET * 2.0))
            .occlude()
            .on_drag_move(move |event: &DragMoveEvent<ThumbDrag>, window, cx| {
                // Where the thumb's top should be, as a share of the track.
                let grab = *grab_read.read(cx);
                let wanted = f32::from(event.event.position.y - track_top - grab) - INSET;
                let track = viewport - INSET * 2.0 - height;
                let progress = if track > 0.0 {
                    (wanted / track).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                drag_state.set_offset_from_scrollbar(point(px(0.0), px(-progress * max_offset)));
                window.refresh();
            })
            .on_mouse_up_out(gpui::MouseButton::Left, move |_, _, _| {
                end_state.scrollbar_drag_ended()
            })
            .on_mouse_up(gpui::MouseButton::Left, {
                let state = self.state.clone();
                move |_, _, _| state.scrollbar_drag_ended()
            })
            .child(
                div()
                    .id("scrollbar-thumb")
                    .absolute()
                    .right(px(INSET))
                    .top(px(top))
                    .w(px(WIDTH))
                    .h(px(height))
                    .rounded_full()
                    .bg(color)
                    .opacity(if self.visible { 1.0 } else { 0.0 })
                    .hover(|style| style.opacity(1.0))
                    .on_drag(ThumbDrag, move |_, offset, _, cx| {
                        start_state.scrollbar_drag_started();
                        grab.update(cx, |grab, _| *grab = offset.y);
                        cx.new(|_| GrabMarker)
                    })
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
    }
}

/// The (invisible) view a drag carries.
struct GrabMarker;

impl Render for GrabMarker {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[cfg(test)]
mod tests {
    use super::thumb_geometry;

    #[test]
    fn the_thumb_tracks_the_scroll() {
        assert_eq!(thumb_geometry(400.0, 0.0, 0.0), None, "nothing to scroll");
        let (top, height) = thumb_geometry(400.0, 400.0, 0.0).unwrap();
        assert_eq!(top, 2.0);
        assert!((height - 198.0).abs() < 0.01);
        let (bottom, _) = thumb_geometry(400.0, 400.0, 400.0).unwrap();
        assert!((bottom + height - 398.0).abs() < 0.01);
        let (_, small) = thumb_geometry(400.0, 100_000.0, 0.0).unwrap();
        assert_eq!(small, 24.0, "a minimum size to grab");
    }
}
