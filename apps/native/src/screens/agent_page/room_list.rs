//! The room's conversation as a virtualized list (`gpui::list`): only the
//! rows on screen are built each frame, and the heights of the others are
//! kept. The rows (`room_rows`) are rebuilt when the room is notified (its
//! data moved: the sidebar, the feeds, the globals it observes), not on a
//! frame the shared clock asked for (a spinner turning) nor on one the views
//! around it drew (the team column's mascots), and only the span that
//! changed is measured again (`splice`), as the task's conversation does. It opens at the bottom and keeps to it while the
//! person reads there; a switch of room or agent starts it over.

use std::ops::Range;
use std::rc::Rc;

use gpui::{FollowMode, ListAlignment, ListState, px};

use super::room_rows::{RoomModel, Row, Scope};

/// Laid out past the edges, so a scroll does not pop rows in.
const OVERDRAW: f32 = 600.0;

pub(crate) struct RoomList {
    pub(crate) state: ListState,
    model: Rc<RoomModel>,
    built: bool,
    /// The room was notified for something other than the clock's tick.
    dirty: bool,
    /// The next notify only redraws (the trail under the pointer): the rows
    /// stay as built.
    held: bool,
    /// Rows built since the room opened, for the tests.
    #[cfg(test)]
    pub(crate) rows_built: std::cell::Cell<usize>,
    #[cfg(test)]
    pub(crate) builds: usize,
}

impl Default for RoomList {
    fn default() -> Self {
        let state = ListState::new(0, ListAlignment::Bottom, px(OVERDRAW));
        state.set_follow_mode(FollowMode::Tail);
        Self {
            state,
            model: Rc::new(RoomModel::default()),
            built: false,
            dirty: false,
            held: false,
            #[cfg(test)]
            rows_built: Default::default(),
            #[cfg(test)]
            builds: 0,
        }
    }
}

impl RoomList {
    /// The room was notified; `tick` when the shared clock did it alone.
    pub(crate) fn notified(&mut self, tick: bool) {
        let held = std::mem::take(&mut self.held);
        self.dirty |= !(tick || held);
    }

    /// The coming notify changes nothing in the rows (`notified`).
    pub(crate) fn hold_rows(&mut self) {
        self.held = true;
    }

    /// Whether this frame must read the room's data again: it was notified
    /// for a change, or it shows another room, agent or set of rooms than
    /// the rows were built for. A frame drawn for any other reason (a tick,
    /// the views around it, a resize) keeps them.
    pub(crate) fn needs_build(&mut self, scope: &Scope) -> bool {
        let dirty = std::mem::take(&mut self.dirty);
        !self.built || dirty || self.model.scope != *scope
    }

    pub(crate) fn model(&self) -> Rc<RoomModel> {
        self.model.clone()
    }

    /// Takes `next`: a new room or agent starts over at the bottom; the same
    /// one measures again only the rows that changed.
    pub(crate) fn take(&mut self, next: RoomModel) {
        #[cfg(test)]
        {
            self.builds += 1;
        }
        // Rows plus the live part under them (`room_items::tail`).
        let count = next.rows.len() + 1;
        let fresh = !self.built || !same_place(&self.model.scope, &next.scope);
        if fresh {
            self.state.reset(count);
            self.state.set_follow_mode(FollowMode::Tail);
        } else {
            let (span, added) = changed_span(&self.model.rows, &next.rows, Row::same);
            if span.start != span.end || added > 0 {
                self.state.splice(span, added);
            }
            // The live part follows whatever runs now: measured again.
            self.state.remeasure_items(next.rows.len()..count);
        }
        self.model = Rc::new(next);
        self.built = true;
    }

    /// One row changed height on its own (an answer opened in place).
    pub(crate) fn remeasure_row(&self, index: usize) {
        self.state.remeasure_items(index..index + 1);
    }

    /// The live part under the rows moved on its own (a turn at work):
    /// measured again, the rows kept (`room_live_poll`).
    pub(crate) fn remeasure_tail(&self) {
        if !self.built {
            return;
        }
        let tail = self.model.rows.len();
        self.state.remeasure_items(tail..tail + 1);
    }

    /// Back to the bottom, following it (the person just sent).
    pub(crate) fn follow(&self) {
        self.state.set_follow_mode(FollowMode::Tail);
    }
}

/// The same room and agent: only the rooms with a project may differ.
fn same_place(a: &Scope, b: &Scope) -> bool {
    a.room == b.room && a.focus == b.focus
}

/// The rows of `old` that `new` replaces, and how many it has there: what
/// both start and end with stays measured.
pub(crate) fn changed_span<T>(
    old: &[T],
    new: &[T],
    same: impl Fn(&T, &T) -> bool,
) -> (Range<usize>, usize) {
    let prefix = old.iter().zip(new).take_while(|(a, b)| same(a, b)).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| same(a, b))
        .count();
    (prefix..old.len() - suffix, new.len() - prefix - suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_rows_that_changed_are_measured_again() {
        let same = |a: &u32, b: &u32| a == b;
        assert_eq!(changed_span(&[1, 2, 3], &[1, 2, 3, 4], same), (3..3, 1));
        assert_eq!(changed_span(&[1, 2, 3], &[1, 9, 3], same), (1..2, 1));
        assert_eq!(changed_span(&[1, 2, 3], &[0, 1, 2, 3], same), (0..0, 1));
        assert_eq!(changed_span(&[1, 2, 3], &[1, 3], same), (1..2, 0));
        assert_eq!(changed_span::<u32>(&[], &[], same), (0..0, 0));
        assert_eq!(changed_span(&[1, 1], &[1, 1, 1], same), (2..2, 1));
    }

    #[test]
    fn only_a_change_reads_the_rows_again() {
        let mut list = RoomList::default();
        let scope = Scope::default();
        assert!(list.needs_build(&scope), "nothing built yet");
        list.take(RoomModel::default());
        // A frame nobody notified the room for (the views around it).
        assert!(!list.needs_build(&scope));
        list.notified(true);
        assert!(!list.needs_build(&scope), "a tick alone");
        list.notified(true);
        list.notified(false);
        list.notified(true);
        assert!(list.needs_build(&scope), "a change in the same frame wins");
        assert!(!list.needs_build(&scope), "read once");
        let other = Scope {
            room: Some("checkout".into()),
            ..Scope::default()
        };
        assert!(list.needs_build(&other), "another room");
    }

    #[test]
    fn the_trail_under_the_pointer_redraws_without_reading_the_rows() {
        let mut list = RoomList::default();
        let scope = Scope::default();
        list.take(RoomModel::default());
        list.hold_rows();
        list.notified(false);
        assert!(!list.needs_build(&scope), "the trail moved, nothing else");
        list.notified(false);
        assert!(list.needs_build(&scope), "the next change reads them again");
    }
}
