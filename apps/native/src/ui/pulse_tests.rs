use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, IntoElement, Render, TestAppContext, Window, div, prelude::*};

use super::*;
use crate::ui::pulse_loop;

#[test]
fn a_lease_ends_once_its_view_stops_drawing_a_loop() {
    let mut leases = Leases::<u32>::default();
    leases.lease(1, Rate::Smooth);
    assert_eq!(leases.tick(false), vec![1]);
    // Not drawn again since: the next due tick lets it go.
    assert!(leases.tick(false).is_empty());
    assert!(leases.is_empty(), "the clock may park");
}

#[test]
fn each_rate_and_a_hidden_app_set_the_pace() {
    let mut leases = Leases::<u32>::default();
    let mut due = [0usize; 3];
    for _ in 0..8 {
        leases.lease(1, Rate::Smooth);
        leases.lease(2, Rate::Half);
        leases.lease(3, Rate::Low);
        for view in leases.tick(false) {
            due[view as usize - 1] += 1;
        }
    }
    assert_eq!(due, [8, 4, 2]);

    // The fastest loop of a view sets its pace.
    let mut leases = Leases::<u32>::default();
    leases.lease(1, Rate::Low);
    leases.lease(1, Rate::Smooth);
    assert_eq!(leases.tick(false), vec![1]);

    // With no window on screen, eight times slower.
    let mut leases = Leases::<u32>::default();
    let mut hidden = 0;
    for _ in 0..(HIDDEN_STRIDE * 2) {
        leases.lease(1, Rate::Smooth);
        hidden += leases.tick(true).len();
    }
    assert_eq!(hidden, 2);
}

struct Spinning {
    on: bool,
    renders: Count,
    ticks: Count,
}

impl Render for Spinning {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_4().when(self.on, |area| {
            area.child(pulse_loop(
                "spin",
                Duration::from_secs(1),
                Rate::Smooth,
                |_| div(),
            ))
        })
    }
}

/// A counter shared with the view.
type Count = Rc<Cell<usize>>;

/// A spinning view, with its renders and the ticks among them.
fn spinning(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<Spinning>,
    &mut gpui::VisualTestContext,
    Count,
    Count,
) {
    cx.update(|cx| cx.set_reduce_motion(false));
    let renders = Rc::new(Cell::new(0));
    let ticks = Rc::new(Cell::new(0));
    let (view, cx) = cx.add_window_view({
        let (renders, ticks) = (renders.clone(), ticks.clone());
        move |_, cx| {
            cx.observe_self(|view: &mut Spinning, cx| {
                if took_tick(cx.entity_id(), cx) {
                    view.ticks.set(view.ticks.get() + 1);
                }
            })
            .detach();
            Spinning {
                on: true,
                renders,
                ticks,
            }
        }
    });
    (view, cx, renders, ticks)
}

#[gpui::test]
fn a_loop_redraws_on_the_clock_and_the_clock_parks_once_it_is_gone(cx: &mut TestAppContext) {
    let (view, cx, renders, ticks) = spinning(cx);
    cx.run_until_parked();
    let first = renders.get();
    cx.executor().advance_clock(TICK * 3);
    cx.run_until_parked();
    assert!(renders.get() > first, "a leased view redraws on ticks");
    assert_eq!(
        ticks.get(),
        renders.get() - first,
        "each of those notifies came from the clock alone"
    );

    let (drawn, ticked) = (renders.get(), ticks.get());
    view.update(cx, |view, cx| {
        view.on = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(renders.get(), drawn + 1);
    assert_eq!(ticks.get(), ticked, "a real notify is no tick");
    cx.executor().advance_clock(TICK * 4);
    cx.run_until_parked();
    let stopped = renders.get();
    cx.executor().advance_clock(TICK * 20);
    cx.run_until_parked();
    assert_eq!(renders.get(), stopped, "a loop not drawn lets the clock go");
    cx.update(|_, cx| assert!(!running(cx)));
}

#[gpui::test]
fn a_hidden_window_slows_its_loops_down(cx: &mut TestAppContext) {
    let (_view, cx, renders, _) = spinning(cx);
    cx.run_until_parked();
    let start = renders.get();
    cx.executor().advance_clock(TICK * HIDDEN_STRIDE as u32);
    cx.run_until_parked();
    assert_eq!(renders.get() - start, HIDDEN_STRIDE as usize);

    cx.simulate_visibility_change(gpui::WindowVisibility::Hidden);
    cx.run_until_parked();
    let hidden = renders.get();
    cx.executor().advance_clock(TICK * HIDDEN_STRIDE as u32 * 2);
    cx.run_until_parked();
    assert!(
        renders.get() - hidden <= 2,
        "a hidden app's loops slow down"
    );
}

#[gpui::test]
fn in_the_back_or_with_reduced_motion_nothing_ticks(cx: &mut TestAppContext) {
    let (_view, cx, renders, _) = spinning(cx);
    cx.update(|_, cx| {
        cx.set_global(crate::data::foreground::Foreground(false));
        cx.refresh_windows();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(TICK * 4);
    cx.run_until_parked();
    let resting = renders.get();
    cx.executor().advance_clock(TICK * 20);
    cx.run_until_parked();
    assert_eq!(renders.get(), resting, "DCC in the back holds its loops");
    cx.update(|_, cx| assert!(!running(cx)));

    cx.update(|_, cx| {
        cx.set_global(crate::data::foreground::Foreground(true));
        cx.set_reduce_motion(true);
        cx.refresh_windows();
    });
    cx.run_until_parked();
    cx.executor().advance_clock(TICK * 20);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!running(cx), "reduced motion leases nothing"));
}
