//! The room keeping up with a turn at work, as the task conversation does
//! (`REFRESH_LIVE`): only while a conversation of the open room has an open
//! turn and the room is drawn, its new events are read every `LIVE_BEAT`
//! off the UI thread (`team_room_poll`), and the loop ends as soon as none
//! runs or the room stops drawing; nothing ticks otherwise. A beat that only
//! moved the open turn redraws the live part under the rows (the list's
//! tail) and never rebuilds the rows themselves (`room_list`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{App, Context};

use super::room::TeamRoom;
use super::room_live_text::LiveText;
use crate::data::agents::AgentState;
use crate::data::team_room::HomeFeed;
use crate::data::team_room_poll::{LivePoll, LiveProbe, Polled};

/// Between two reads while DCC is in front.
pub(crate) const LIVE_BEAT: Duration = Duration::from_millis(700);
/// With DCC in the back: what moved is there when it comes back.
const LIVE_BEAT_BACK: Duration = Duration::from_millis(2500);
/// A room not drawn for this long is not looked at: the loop stops and the
/// next frame it draws starts it again.
const DRAWN_WITHIN: Duration = Duration::from_secs(2);

/// The live part's own state.
#[derive(Default)]
pub(crate) struct RoomLive {
    /// The loop runs.
    watching: bool,
    /// The next notify only moved the live tail (read by `observe_self`).
    pub(crate) quiet: bool,
    drawn_at: Option<Instant>,
    /// Conversations asked to stop, until their turn ends.
    pub(crate) stopping: HashSet<String>,
    /// Stopped turns sent again from here, so their row leaves at once.
    pub(crate) retried: HashSet<String>,
    /// The answers streaming, by conversation (`room_live_text`).
    pub(crate) texts: RefCell<HashMap<String, LiveText>>,
    /// The own conversations of the tasks running here, read for their
    /// open turn only (`room_task_live`).
    pub(crate) tasks: Vec<HomeFeed>,
    /// What each task row drew last (clock, label), for the tests.
    #[cfg(test)]
    pub(crate) drawn: RefCell<HashMap<String, (String, String)>>,
    /// Beats that read the database, for the tests.
    #[cfg(test)]
    pub(crate) polls: usize,
}

impl TeamRoom {
    /// The conversations of this room with a turn at work now: open in their
    /// events and working as the sidebar sees them (a turn that ended with no
    /// output never keeps one live).
    pub(crate) fn live_feeds(&self, cx: &App) -> Vec<&HomeFeed> {
        let agents = self.sidebar.read(cx).agents();
        self.feeds
            .iter()
            .filter(|feed| {
                feed.now.live().is_some()
                    && self.shows(&feed.agent_id)
                    && agents.iter().any(|agent| {
                        agent.id == feed.agent_id
                            && agent.sessions.iter().any(|session| {
                                session.session_id == feed.session_id
                                    && session.state == AgentState::Working
                            })
                    })
            })
            .collect()
    }

    /// Called on each frame the room draws: starts the loop while one of
    /// its conversations works. Tests feed the room themselves (no
    /// database), so nothing is read there.
    pub(super) fn watch_live(&mut self, cx: &mut Context<Self>) {
        self.live.drawn_at = Some(Instant::now());
        if self.live.watching
            || self.db_path.as_os_str().is_empty()
            || (self.live_feeds(cx).is_empty() && self.followed_tasks(cx).is_empty())
        {
            return;
        }
        self.live.watching = true;
        cx.spawn(async move |this, cx| {
            let mut beat = LIVE_BEAT;
            loop {
                cx.background_executor().timer(beat).await;
                let Ok(Some((probes, db, generation, next))) =
                    this.update(cx, |room, cx| room.live_due(cx))
                else {
                    return;
                };
                beat = next;
                let read: Vec<LivePoll> = cx
                    .background_executor()
                    .spawn(async move {
                        probes
                            .into_iter()
                            .map(|probe: LiveProbe| probe.poll(&db))
                            .collect()
                    })
                    .await;
                if this
                    .update(cx, |room, cx| room.take_live(read, generation, cx))
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    /// What this beat reads, or none: the loop ends.
    fn live_due(&mut self, cx: &App) -> Option<(Vec<LiveProbe>, PathBuf, u64, Duration)> {
        let drawn = self
            .live
            .drawn_at
            .is_some_and(|at| at.elapsed() < DRAWN_WITHIN);
        let mut probes: Vec<LiveProbe> = if drawn {
            self.live_feeds(cx)
                .into_iter()
                .map(HomeFeed::probe)
                .collect()
        } else {
            Vec::new()
        };
        if drawn {
            probes.extend(self.task_probes(cx));
        }
        if probes.is_empty() {
            self.live.watching = false;
            return None;
        }
        let beat = if crate::data::foreground::is_foreground(cx) {
            LIVE_BEAT
        } else {
            LIVE_BEAT_BACK
        };
        Some((probes, self.db_path.clone(), self.room_generation, beat))
    }

    /// Takes a beat's reads: a turn that ended (or anything beyond its own
    /// work) rebuilds the rows as any change does; work inside the turn only
    /// redraws the tail.
    pub(crate) fn take_live(
        &mut self,
        read: Vec<LivePoll>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        {
            self.live.polls += 1;
        }
        if generation != self.room_generation {
            return;
        }
        let mut moved = Polled::Same;
        for poll in read {
            let session = poll.session_id().to_string();
            let Some(feed) = self
                .feeds
                .iter_mut()
                .chain(self.live.tasks.iter_mut())
                .find(|feed| feed.session_id == session)
            else {
                continue;
            };
            match feed.take_poll(poll) {
                Polled::Lines => moved = Polled::Lines,
                Polled::Turn if moved == Polled::Same => moved = Polled::Turn,
                _ => {}
            }
        }
        self.forget_settled();
        if moved == Polled::Lines {
            // As a read of the sidebar's would: what was on its way leaves,
            // the cards a turn proposed show.
            self.take(Vec::new());
            self.refresh_proposed(cx);
            cx.notify();
        } else if moved == Polled::Turn || cx.reduce_motion() {
            // The clock moves on the shared one; with reduced motion only
            // these beats move it.
            self.live.quiet = true;
            self.list.remeasure_tail();
            cx.notify();
        }
    }

    /// Drops what the live part kept for turns no longer at work.
    pub(crate) fn forget_settled(&mut self) {
        let live: HashMap<&str, &str> = self
            .feeds
            .iter()
            .filter_map(|feed| {
                feed.now
                    .live()
                    .map(|turn| (feed.session_id.as_str(), turn.turn_id.as_str()))
            })
            .collect();
        self.live
            .texts
            .borrow_mut()
            .retain(|session, text| live.get(session.as_str()) == Some(&text.turn_id.as_str()));
        self.live
            .stopping
            .retain(|session| live.contains_key(session.as_str()));
    }
}
