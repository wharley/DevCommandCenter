//! One shared clock for decorative loops (spinners, a working mascot's bob),
//! after the Monocode's `pulse.rs`.
//!
//! A view that draws a loop leases the clock for itself while it draws it.
//! Each tick notifies every leased view together, so all loops land in one
//! frame instead of one timer per spinner. A lease ends when its view was
//! notified and did not draw a loop again, so a closed or scrolled-away
//! spinner costs nothing from the next tick on, and the clock's one timer
//! stops once no lease is left. With no window on screen the loops tick
//! eight times slower; with DCC in the back they hold still, as `animated`
//! does (`data::foreground`).
//!
//! A view that keeps work derived from its data between frames (the team
//! room's rows) can tell a tick from a real change with [`took_tick`].

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::{Duration, Instant};

use gpui::{App, EntityId, Global, Window};

/// About 30 frames a second, the fastest any loop draws.
pub const TICK: Duration = Duration::from_millis(33);
/// With no window on screen, loops tick this many times slower.
const HIDDEN_STRIDE: u64 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rate {
    /// About 30 fps: motion that must look continuous.
    Smooth,
    /// About 15 fps: small loaders and the bob (`LOOP_FPS` of `animated`).
    Half,
    /// About 7.5 fps: pixel art stepping between a few poses.
    Low,
}

impl Rate {
    fn stride(self) -> u64 {
        match self {
            Rate::Smooth => 1,
            Rate::Half => 2,
            Rate::Low => 4,
        }
    }
}

#[derive(Debug)]
struct Lease {
    rate: Rate,
    /// Drew a loop since the clock last notified it.
    painted: bool,
}

/// Who leases the clock, apart from GPUI so the rules test alone.
#[derive(Debug)]
pub(crate) struct Leases<K> {
    leases: HashMap<K, Lease>,
    tick: u64,
}

impl<K> Default for Leases<K> {
    fn default() -> Self {
        Self {
            leases: HashMap::new(),
            tick: 0,
        }
    }
}

impl<K: Copy + Eq + Hash> Leases<K> {
    /// `view` drew a loop at `rate`; the fastest loop of a view sets its pace.
    pub(crate) fn lease(&mut self, view: K, rate: Rate) {
        let lease = self.leases.entry(view).or_insert(Lease {
            rate,
            painted: true,
        });
        lease.painted = true;
        if rate.stride() < lease.rate.stride() {
            lease.rate = rate;
        }
    }

    /// One tick: the views to redraw now. A view due that drew no loop since
    /// its last redraw leaves; `hidden` stretches every pace.
    pub(crate) fn tick(&mut self, hidden: bool) -> Vec<K> {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;
        let mut due = Vec::new();
        self.leases.retain(|view, lease| {
            let stride = lease.rate.stride() * if hidden { HIDDEN_STRIDE } else { 1 };
            if !tick.is_multiple_of(stride) {
                return true;
            }
            if !lease.painted {
                return false;
            }
            lease.painted = false;
            due.push(*view);
            true
        });
        due
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }
}

#[derive(Default)]
struct Clock {
    epoch: Option<Instant>,
    leases: Leases<EntityId>,
    /// Views the last tick notified, until they ask (`took_tick`).
    ticked: HashSet<EntityId>,
    running: bool,
}

impl Global for Clock {}

/// Keeps the view being drawn redrawing at `rate`. Call it each time a loop
/// is drawn. False, leasing nothing, under reduced motion or with DCC in the
/// back: draw the loop's resting frame then.
pub fn request(rate: Rate, window: &Window, cx: &mut App) -> bool {
    if cx.reduce_motion() || !crate::data::foreground::is_foreground(cx) {
        return false;
    }
    let view = window.current_view();
    let clock = cx.default_global::<Clock>();
    clock.leases.lease(view, rate);
    if !clock.running {
        clock.running = true;
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if !cx.update(tick) {
                    break;
                }
            }
        })
        .detach();
    }
    true
}

/// The fraction of `period` the shared clock is into, in `0..1`: loops of
/// one period stay in step.
pub fn phase(period: Duration, cx: &mut App) -> f32 {
    if period.is_zero() {
        return 0.0;
    }
    let now = cx.background_executor().now();
    let clock = cx.default_global::<Clock>();
    let elapsed = now - *clock.epoch.get_or_insert(now);
    (elapsed.as_nanos() % period.as_nanos()) as f32 / period.as_nanos() as f32
}

/// Whether the notify `view` just got came from the clock alone. Ask it from
/// the view's own `observe_self`; it answers once per tick.
pub fn took_tick(view: EntityId, cx: &mut App) -> bool {
    cx.has_global::<Clock>() && cx.global_mut::<Clock>().ticked.remove(&view)
}

/// Whether the clock's timer is on.
#[cfg(test)]
pub(crate) fn running(cx: &App) -> bool {
    cx.try_global::<Clock>().is_some_and(|clock| clock.running)
}

fn tick(cx: &mut App) -> bool {
    let hidden = !any_window_visible(cx);
    let clock = cx.global_mut::<Clock>();
    // The last tick's observers already ran: its marks are stale now.
    clock.ticked.clear();
    let due = clock.leases.tick(hidden);
    if clock.leases.is_empty() {
        clock.running = false;
        return false;
    }
    clock.ticked.extend(due.iter().copied());
    for view in due {
        cx.notify(view);
    }
    true
}

/// Whether any window's frames are shown (not minimized, not covered).
fn any_window_visible(cx: &mut App) -> bool {
    cx.windows().into_iter().any(|window| {
        window
            .update(cx, |_, window, _| window.is_visible())
            .unwrap_or(false)
    })
}

#[cfg(test)]
#[path = "pulse_tests.rs"]
mod tests;
