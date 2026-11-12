//! The trail down a thread's left gutter (`ConversationTrail`): a tick per
//! stop; ticks near the pointer grow (a gaussian, like a dock), the one
//! under it shows its message and answer beside the trail, and a click goes
//! there. The tick of the stop being read stays brighter. The task's
//! conversation and the team room both draw it.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, Bounds, BoxShadow, Context, MouseMoveEvent, Pixels, ScrollHandle, anchored,
    canvas, deferred, div, point, prelude::*, px, relative,
};

use crate::theme::ActiveTheme;

const TICK_SPACING: f32 = 10.0;
const TICK_BASE_WIDTH: f32 = 6.0;
const TICK_MAX_WIDTH: f32 = 30.0;
const TICK_SIGMA: f32 = 14.0;
/// `w-14`: the gutter the ticks sit in.
pub const GUTTER_WIDTH: f32 = 56.0;
pub const MAX_PREVIEW: usize = 280;

/// One stop: a message and the answer it got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrailItem {
    /// The message's row in its list.
    pub row: usize,
    pub prompt: String,
    pub response: String,
}

/// A trail's own state, on the view that draws it.
#[derive(Default)]
pub struct TrailState {
    /// The pointer's height over the track, while it is over it.
    pointer: Option<f32>,
    /// Where the track was painted (it moves with the trail's scroll).
    track: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: ScrollHandle,
    /// The tick last kept in view: the trail follows the thread only when
    /// the stop being read changes.
    followed: Option<usize>,
}

/// A message on one line, cut at `MAX_PREVIEW`.
pub fn preview(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > MAX_PREVIEW {
        let cut: String = collapsed.chars().take(MAX_PREVIEW).collect();
        format!("{}…", cut.trim_end())
    } else {
        collapsed
    }
}

/// `focusedTrailIndex`: the tick nearest to a height on the track.
pub fn focused_index(pointer: f32, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    ((pointer / TICK_SPACING).round().max(0.0) as usize).min(count - 1)
}

/// `trailMagnificationWeights`: how much each tick grows, 1 under the pointer.
pub fn weights(count: usize, pointer: f32) -> Vec<f32> {
    let denominator = 2.0 * TICK_SIGMA * TICK_SIGMA;
    (0..count)
        .map(|index| {
            let distance = index as f32 * TICK_SPACING - pointer;
            (-(distance * distance) / denominator).exp()
        })
        .collect()
}

/// The trail of `items` for a view `V`: `state` reaches its `TrailState`,
/// `current` is the stop being read, `go` takes the view to a stop. Nothing
/// below two stops.
pub fn trail<V: 'static>(
    id: &'static str,
    items: Rc<Vec<TrailItem>>,
    current: usize,
    viewport_width: f32,
    state: fn(&mut V) -> &mut TrailState,
    go: fn(&mut V, &TrailItem, &mut Context<V>),
    view: &mut V,
    cx: &mut Context<V>,
) -> Option<AnyElement> {
    if items.len() < 2 {
        return None;
    }
    let trail = state(view);
    let colors = cx.theme().colors.clone();
    if trail.followed != Some(current) {
        trail.followed = Some(current);
        let tick = current as f32 * TICK_SPACING;
        let shown = trail.scroll.bounds().size.height;
        let top = -trail.scroll.offset().y;
        if px(tick) < top {
            trail.scroll.set_offset(point(px(0.0), px(-tick)));
        } else if px(tick + 2.0) > top + shown {
            trail
                .scroll
                .set_offset(point(px(0.0), shown - px(tick + 2.0)));
        }
    }
    let pointer = trail.pointer;
    let active = pointer.map(|pointer| focused_index(pointer, items.len()));
    let grow = pointer.map(|pointer| weights(items.len(), pointer));
    let ticks = (0..items.len()).map(|index| {
        let weight = grow.as_ref().map_or(0.0, |grow| grow[index]);
        let opacity = if active == Some(index) {
            1.0
        } else if index == current {
            0.9
        } else {
            0.24
        };
        div()
            .absolute()
            .top(px(index as f32 * TICK_SPACING))
            .left(px(14.0))
            .h(px(2.0))
            .w(px(
                TICK_BASE_WIDTH + (TICK_MAX_WIDTH - TICK_BASE_WIDTH) * weight
            ))
            .rounded_full()
            .bg(colors.foreground.opacity(opacity))
    });
    let track_cell = trail.track.clone();
    let track = div()
        .relative()
        .w_full()
        .h(px((items.len() - 1) as f32 * TICK_SPACING + 2.0))
        .children(ticks)
        .child(
            canvas(
                move |bounds, _, _| track_cell.set(Some(bounds)),
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
    let card = active.zip(trail.track.get()).and_then(|(index, track)| {
        let item = items.get(index)?;
        let width = (viewport_width - 80.0).clamp(160.0, 320.0);
        let at = point(
            track.left() + px(64.0),
            track.top() + px(index as f32 * TICK_SPACING + 1.0),
        );
        let card = div()
            .debug_selector(move || format!("{id}-preview"))
            .w(px(width))
            .max_h(px(136.0))
            .overflow_hidden()
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(colors.border.opacity(0.82))
            .bg(colors.popover.opacity(0.96))
            .shadow(vec![BoxShadow {
                color: gpui::Hsla::black().opacity(0.18),
                offset: point(px(0.0), px(14.0)),
                blur_radius: px(36.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .text_size(px(12.0))
            .line_height(px(16.0))
            .child(
                div()
                    .line_clamp(2)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(item.prompt.clone()),
            )
            .when(!item.response.is_empty(), |card| {
                card.child(
                    div()
                        .mt(px(5.0))
                        .line_clamp(3)
                        .text_color(colors.muted_foreground)
                        .child(item.response.clone()),
                )
            });
        Some(
            deferred(
                anchored()
                    .position(at)
                    .anchor(Anchor::LeftCenter)
                    .snap_to_window_with_margin(px(12.0))
                    .child(card),
            )
            .priority(2),
        )
    });
    let move_items = items.clone();
    let viewport = div()
        .id(id)
        .debug_selector(move || id.to_string())
        .relative()
        .w_full()
        .max_h(relative(0.8))
        .overflow_y_scroll()
        .track_scroll(&trail.scroll)
        .cursor_pointer()
        .on_mouse_move(cx.listener(move |view, event: &MouseMoveEvent, _, cx| {
            let trail = state(view);
            if let Some(track) = trail.track.get() {
                trail.pointer = Some(f32::from(event.position.y - track.top()));
                cx.notify();
            }
        }))
        .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
            if !*hovered {
                state(view).pointer = None;
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
            let Some(track) = state(view).track.get() else {
                return;
            };
            let at = f32::from(event.position().y - track.top());
            let index = focused_index(at, move_items.len());
            if let Some(item) = move_items.get(index) {
                go(view, item, cx);
                cx.notify();
            }
        }))
        .child(track);
    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(GUTTER_WIDTH))
            .flex()
            .flex_col()
            .justify_center()
            .child(viewport)
            .children(card)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_previews_are_cut() {
        let prompt = preview(&"x".repeat(400));
        assert_eq!(prompt.chars().count(), MAX_PREVIEW + 1);
        assert!(prompt.ends_with('…'));
    }

    #[test]
    fn the_pointer_picks_and_grows_the_nearest_tick() {
        assert_eq!(focused_index(-5.0, 4), 0);
        assert_eq!(focused_index(14.0, 4), 1);
        assert_eq!(focused_index(500.0, 4), 3);
        let grow = weights(3, 10.0);
        assert!((grow[1] - 1.0).abs() < f32::EPSILON);
        assert!(grow[0] < 1.0 && grow[0] > 0.0 && (grow[0] - grow[2]).abs() < 1e-6);
    }
}
