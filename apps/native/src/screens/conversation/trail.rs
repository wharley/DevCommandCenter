//! `ConversationTrail`: a tick per prompt down the thread's left gutter.
//! Ticks near the pointer grow (a gaussian, like a dock), the one under it
//! shows its prompt and answer beside the trail, and a click scrolls the
//! thread to that prompt. The tick of the prompt being read stays brighter.

use std::cell::Cell;
use std::rc::Rc;

use dcc_core::domain::timeline::{Role, TimelineMessage};
use gpui::{
    Anchor, AnyElement, Bounds, BoxShadow, Context, ListOffset, MouseMoveEvent, Pixels,
    ScrollHandle, anchored, canvas, deferred, div, point, prelude::*, px, relative,
};

use super::ConversationView;
use crate::theme::ActiveTheme;

const TICK_SPACING: f32 = 10.0;
const TICK_BASE_WIDTH: f32 = 6.0;
const TICK_MAX_WIDTH: f32 = 30.0;
const TICK_SIGMA: f32 = 14.0;
/// `w-14`: the gutter the ticks sit in.
const GUTTER_WIDTH: f32 = 56.0;
const MAX_PREVIEW: usize = 280;

/// One stop: a prompt and the answer its turn gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TrailItem {
    /// The prompt's row in the conversation.
    pub row: usize,
    pub prompt: String,
    pub response: String,
}

/// The trail's own state on the conversation.
#[derive(Default)]
pub(super) struct TrailState {
    /// The pointer's height over the track, while it is over it.
    pointer: Option<f32>,
    /// Where the track was painted (it moves with the trail's scroll).
    track: Rc<Cell<Option<Bounds<Pixels>>>>,
    scroll: ScrollHandle,
    /// The tick last kept in view: the trail follows the thread only when
    /// the prompt being read changes.
    followed: Option<usize>,
}

fn preview(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > MAX_PREVIEW {
        let cut: String = collapsed.chars().take(MAX_PREVIEW).collect();
        format!("{}…", cut.trim_end())
    } else {
        collapsed
    }
}

/// `deriveConversationTrailItems`: one stop per prompt (the backend's own
/// `[DCC]` turns are not the person's), each with the last answer of its
/// turn, or of the prompt before it when the history has no turn ids.
pub(super) fn trail_items(rows: &[TimelineMessage]) -> Vec<TrailItem> {
    let mut items: Vec<TrailItem> = Vec::new();
    let mut turns: Vec<(&str, usize)> = Vec::new();
    for (row, message) in rows.iter().enumerate() {
        match message.role {
            Role::User if message.hand_back.is_none() => {
                if let Some(turn) = message.turn_id.as_deref() {
                    turns.push((turn, items.len()));
                }
                items.push(TrailItem {
                    row,
                    prompt: preview(&message.content),
                    response: String::new(),
                });
            }
            Role::Assistant => {
                let response = preview(&message.content);
                if response.is_empty() {
                    continue;
                }
                let index = message
                    .turn_id
                    .as_deref()
                    .and_then(|turn| turns.iter().rev().find(|(id, _)| *id == turn))
                    .map(|(_, index)| *index)
                    .or_else(|| items.len().checked_sub(1));
                if let Some(item) = index.and_then(|index| items.get_mut(index)) {
                    item.response = response;
                }
            }
            _ => {}
        }
    }
    items
}

/// `focusedTrailIndex`: the tick nearest to a height on the track.
pub(super) fn focused_index(pointer: f32, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    ((pointer / TICK_SPACING).round().max(0.0) as usize).min(count - 1)
}

/// `trailMagnificationWeights`: how much each tick grows, 1 under the pointer.
pub(super) fn weights(count: usize, pointer: f32) -> Vec<f32> {
    let denominator = 2.0 * TICK_SIGMA * TICK_SIGMA;
    (0..count)
        .map(|index| {
            let distance = index as f32 * TICK_SPACING - pointer;
            (-(distance * distance) / denominator).exp()
        })
        .collect()
}

impl ConversationView {
    /// The prompt being read: the last one at or above the thread's top.
    fn trail_current(&self, items: &[TrailItem]) -> usize {
        let top = self.list.logical_scroll_top().item_ix;
        // The list's first item is the top room; row `n` is item `n + 1`.
        items.iter().rposition(|item| item.row < top).unwrap_or(0)
    }

    fn trail_go(&mut self, index: usize, items: &[TrailItem], cx: &mut Context<Self>) {
        if let Some(item) = items.get(index) {
            self.list.scroll_to(ListOffset {
                item_ix: item.row + 1,
                offset_in_item: px(0.0),
            });
            cx.notify();
        }
    }

    /// The trail, or nothing below two prompts.
    pub(super) fn trail(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let data = self.data.clone()?;
        let items = Rc::new(trail_items(&data.rows));
        if items.len() < 2 {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let current = self.trail_current(&items);
        if self.trail.followed != Some(current) {
            self.trail.followed = Some(current);
            let tick = current as f32 * TICK_SPACING;
            let shown = self.trail.scroll.bounds().size.height;
            let top = -self.trail.scroll.offset().y;
            if px(tick) < top {
                self.trail.scroll.set_offset(point(px(0.0), px(-tick)));
            } else if px(tick + 2.0) > top + shown {
                self.trail
                    .scroll
                    .set_offset(point(px(0.0), shown - px(tick + 2.0)));
            }
        }
        let pointer = self.trail.pointer;
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
        let track_cell = self.trail.track.clone();
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
        let card = active
            .zip(self.trail.track.get())
            .and_then(|(index, track)| {
                let item = items.get(index)?;
                let width = (self.viewport_width - 80.0).clamp(160.0, 320.0);
                let at = point(
                    track.left() + px(64.0),
                    track.top() + px(index as f32 * TICK_SPACING + 1.0),
                );
                let card = div()
                    .debug_selector(|| "conversation-trail-preview".into())
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
            .id("conversation-trail")
            .debug_selector(|| "conversation-trail".into())
            .relative()
            .w_full()
            .max_h(relative(0.8))
            .overflow_y_scroll()
            .track_scroll(&self.trail.scroll)
            .cursor_pointer()
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if let Some(track) = view.trail.track.get() {
                    view.trail.pointer = Some(f32::from(event.position.y - track.top()));
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered {
                    view.trail.pointer = None;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                let Some(track) = view.trail.track.get() else {
                    return;
                };
                let at = f32::from(event.position().y - track.top());
                let index = focused_index(at, move_items.len());
                view.trail_go(index, &move_items, cx);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, role: Role, turn: Option<&str>, content: &str) -> TimelineMessage {
        TimelineMessage {
            id: id.into(),
            role,
            turn_id: turn.map(str::to_string),
            turn_settled: true,
            turn_started_at: None,
            turn_ended_at: None,
            assistant_phase: None,
            content: content.into(),
            label: String::new(),
            model: None,
            streaming: false,
            created_at: None,
            status: None,
            annotations: Vec::new(),
            plan_mode: false,
            answers_plan_mode: false,
            evidence: None,
            retry_of_turn_id: None,
            action: None,
            delegation: None,
            delegation_turn_id: None,
            hand_back: None,
        }
    }

    #[test]
    fn a_stop_per_prompt_with_its_turns_answer() {
        let rows = vec![
            row("u1", Role::User, Some("t1"), "Primeira  pergunta\n"),
            row("a1", Role::Assistant, Some("t1"), "resposta um"),
            row("s", Role::System, None, "nota"),
            row("u2", Role::User, Some("t2"), "Segunda"),
            row("a0", Role::Assistant, Some("t1"), "fim do um"),
        ];
        let items = trail_items(&rows);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].row, 0);
        assert_eq!(items[0].prompt, "Primeira pergunta");
        assert_eq!(items[0].response, "fim do um");
        assert_eq!(items[1].row, 3);
        assert_eq!(items[1].response, "");
    }

    #[test]
    fn without_turn_ids_the_answer_goes_to_the_prompt_before_it() {
        let rows = vec![
            row("u1", Role::User, None, "a"),
            row("a1", Role::Assistant, None, "b"),
        ];
        assert_eq!(trail_items(&rows)[0].response, "b");
    }

    #[test]
    fn long_previews_are_cut() {
        let long = "x".repeat(400);
        let rows = vec![row("u1", Role::User, None, &long)];
        let prompt = &trail_items(&rows)[0].prompt;
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
