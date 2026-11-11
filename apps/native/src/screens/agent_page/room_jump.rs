//! "Ir para o fim" (Tucano's way): while the person reads above the end of
//! the room, a round button floats over the field's right side, with how
//! many of the person's and the agents' lines came in since they left the
//! end ("3 novas"); a click goes back down and follows the end again. Read
//! from the list's scroll offset when the room draws, outside its rows: no
//! timer and no rows rebuilt for it.

use gpui::{AnyElement, BoxShadow, Context, SharedString, div, point, prelude::*, px};

use super::room::TeamRoom;
use super::room_rows::Row;
use crate::data::team_room::Speaker;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Icon, Tooltip};

/// How far the person is from the end, as the lines counted then.
#[derive(Default)]
pub(crate) struct Jump {
    /// The lines there were when the person left the end.
    from: Option<usize>,
}

/// The person's and the agents' lines among `rows` (no cards, no turns
/// passed, no routine's or source's lines).
pub(crate) fn talk(rows: &[Row]) -> usize {
    rows.iter()
        .filter(|row| match row {
            Row::Line { line, said, .. } => said.is_some() || line.speaker == Speaker::Agent,
            _ => false,
        })
        .count()
}

/// The lines new since the person left the end, or none at the end. The
/// count to compare with is taken the first time the person is away.
pub(crate) fn unseen(at_end: bool, talk: usize, from: &mut Option<usize>) -> Option<usize> {
    if at_end {
        *from = None;
        return None;
    }
    Some(talk.saturating_sub(*from.get_or_insert(talk)))
}

impl TeamRoom {
    /// Away from the end: the new lines since, else none.
    pub(crate) fn unseen_lines(&mut self) -> Option<usize> {
        let state = &self.list.state;
        let at_end =
            state.is_following_tail() || state.logical_scroll_top().item_ix >= state.item_count();
        unseen(at_end, self.list.model().talk, &mut self.jump.from)
    }

    /// Back to the end, following it; the rows are kept.
    pub(crate) fn jump_to_end(&mut self, cx: &mut Context<Self>) {
        self.list.follow();
        self.jump.from = None;
        self.live.quiet = true;
        cx.notify();
    }

    /// The round button over the field's right side, while away from the end.
    pub(super) fn jump_button(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let unseen = self.unseen_lines()?;
        let colors = cx.theme().colors.clone();
        let count = (unseen > 0).then(|| {
            div()
                .debug_selector(|| "room-jump-count".into())
                .pl(px(2.0))
                .text_size(px(12.0))
                .text_color(colors.foreground)
                .child(cx.t_with("agents.room.newLines", &Args::count(unseen as i64)))
        });
        let tip: SharedString = cx.t("agents.room.jumpTip");
        Some(
            div()
                .absolute()
                .bottom(px(10.0))
                .right(px(24.0))
                .child(Tooltip::text(
                    "room-jump-tip",
                    div()
                        .id("room-jump")
                        .debug_selector(|| "room-jump".into())
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .h(px(32.0))
                        .min_w(px(32.0))
                        .when(count.is_some(), |button| button.pl(px(10.0)).pr(px(8.0)))
                        .when(count.is_none(), |button| button.justify_center())
                        .rounded_full()
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.background)
                        .shadow(vec![BoxShadow {
                            color: gpui::black().opacity(0.08),
                            offset: point(px(0.0), px(4.0)),
                            blur_radius: px(12.0),
                            spread_radius: px(0.0),
                            inset: false,
                        }])
                        .cursor_pointer()
                        .hover(|button| button.bg(colors.accent))
                        .on_click(cx.listener(|room, _, _, cx| room.jump_to_end(cx)))
                        .children(count)
                        .child(
                            Icon::new("arrow-down")
                                .size(15.0)
                                .stroke(2.0)
                                .color(colors.foreground),
                        ),
                    tip,
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_lines_count_from_when_the_person_left_the_end() {
        let mut from = None;
        assert_eq!(unseen(true, 10, &mut from), None);
        assert_eq!(unseen(false, 10, &mut from), Some(0), "just left");
        assert_eq!(unseen(false, 13, &mut from), Some(3));
        assert_eq!(unseen(true, 13, &mut from), None, "back at the end");
        assert_eq!(unseen(false, 14, &mut from), Some(0), "counted again");
        assert_eq!(unseen(false, 12, &mut from), Some(0), "lines gone");
    }
}
