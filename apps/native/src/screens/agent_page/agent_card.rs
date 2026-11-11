//! A small card about an agent (Tucano's `AgentCard`): its face or name in
//! the room's answers, the live rows, the `@` list and the team's column
//! open it on a click, or after half a second of hover, so the person sees
//! who it is, where it runs and what it does now without leaving the room.
//! One card per host at a time; Esc, a click outside, or the pointer away
//! from the card and its face for a moment close it. A hover starts one
//! timer and a leave drops it: nothing runs while nothing is hovered.
//! What the card shows is in `agent_card_view`, built only while it is open.

use std::time::Duration;

use gpui::{
    AnyElement, App, Context, Entity, SharedString, Subscription, Task, Window, anchored, deferred,
    div, point, prelude::*, px,
};

use crate::screens::composer::Composer;
use crate::screens::sidebar::Sidebar;

/// A hover this long opens the card.
pub(crate) const OPEN_DELAY: Duration = Duration::from_millis(500);
/// The pointer away from the card and its face this long closes it.
pub(crate) const CLOSE_DELAY: Duration = Duration::from_millis(300);

/// The card open: the element that opened it and its agent.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardAt {
    pub(crate) trigger: SharedString,
    pub(crate) agent_id: String,
}

/// A host's card: which one is open and the timers of a hover.
#[derive(Default)]
pub(crate) struct AgentCards {
    pub(crate) open: Option<CardAt>,
    /// A hover waiting to open its trigger's card.
    hover: Option<(SharedString, Task<()>)>,
    /// The pointer left: the card closes unless it comes back.
    leave: Option<Task<()>>,
    /// Esc while open, whatever has the focus.
    escape: Option<Subscription>,
}

impl AgentCards {
    pub(crate) fn is_open(&self, trigger: &str) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.trigger.as_ref() == trigger)
    }
}

/// What owns a card: the room or the agent's page.
pub(crate) trait CardHost: Sized + 'static {
    fn cards(&mut self) -> &mut AgentCards;
    fn cards_ref(&self) -> &AgentCards;
    /// Where the card reads the agent, its state and what it runs on.
    fn card_sources(&self) -> (Entity<Sidebar>, Entity<Composer>);
    /// "Mensagem": "@Name " in the room's field, focused.
    fn card_message(&mut self, agent_id: &str, window: &mut Window, cx: &mut Context<Self>);
    /// "Ver perfil": the agent's page on "Configuração".
    fn card_profile(&mut self, agent_id: &str, cx: &mut Context<Self>);
    /// Draws again for the card alone (the room keeps its rows).
    fn card_redraw(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}

/// Opens `trigger`'s card on `agent_id`, closing any other.
pub(crate) fn open<T: CardHost>(
    host: &mut T,
    trigger: SharedString,
    agent_id: String,
    cx: &mut Context<T>,
) {
    let this = cx.weak_entity();
    let escape = cx.intercept_keystrokes(move |event, _, cx| {
        if event.keystroke.key == "escape" {
            let closed = this
                .update(cx, |host, cx| close(host, cx))
                .unwrap_or_default();
            if closed {
                cx.stop_propagation();
            }
        }
    });
    let cards = host.cards();
    cards.hover = None;
    cards.leave = None;
    cards.escape = Some(escape);
    cards.open = Some(CardAt { trigger, agent_id });
    host.card_redraw(cx);
}

/// Closes the card; whether one was open.
pub(crate) fn close<T: CardHost>(host: &mut T, cx: &mut Context<T>) -> bool {
    let cards = host.cards();
    cards.hover = None;
    cards.leave = None;
    cards.escape = None;
    let was = cards.open.take().is_some();
    if was {
        host.card_redraw(cx);
    }
    was
}

/// The pointer left the face or the card: it closes in a moment.
fn leave_soon<T: CardHost>(host: &mut T, cx: &mut Context<T>) {
    if host.cards().open.is_none() {
        return;
    }
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor().timer(CLOSE_DELAY).await;
        let _ = this.update(cx, |host, cx| close(host, cx));
    });
    host.cards().leave = Some(task);
}

fn hover_trigger<T: CardHost>(
    host: &mut T,
    trigger: SharedString,
    agent_id: String,
    hovered: bool,
    cx: &mut Context<T>,
) {
    let cards = host.cards();
    if !hovered {
        if cards.hover.as_ref().is_some_and(|(key, _)| *key == trigger) {
            cards.hover = None;
        }
        if cards.is_open(&trigger) {
            leave_soon(host, cx);
        }
        return;
    }
    if cards.is_open(&trigger) {
        cards.leave = None;
        return;
    }
    let key = trigger.clone();
    let task = cx.spawn(async move |this, cx| {
        cx.background_executor().timer(OPEN_DELAY).await;
        let _ = this.update(cx, |host, cx| open(host, key, agent_id, cx));
    });
    cards.hover = Some((trigger, task));
}

/// Wraps `child` (a face, a name, a row) as what opens `agent_id`'s card:
/// on hover always, on a click when `click` (the `@` list keeps its click
/// for picking). The card, when it is this one's, floats beside it.
pub(crate) fn card_trigger<T: CardHost>(
    host: &T,
    trigger: impl Into<SharedString>,
    agent_id: &str,
    click: bool,
    child: impl IntoElement,
    window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    let trigger: SharedString = trigger.into();
    let shown = host.cards_ref().is_open(&trigger);
    let card = shown.then(|| floating(host, agent_id, window, cx));
    let (key, agent) = (trigger.clone(), agent_id.to_string());
    div()
        .id(SharedString::from(format!("card-trigger-{trigger}")))
        .relative()
        .on_hover(cx.listener(move |host, hovered: &bool, _, cx| {
            hover_trigger(host, key.clone(), agent.clone(), *hovered, cx)
        }))
        .when(click, |wrapper| {
            let (key, agent) = (trigger.clone(), agent_id.to_string());
            wrapper
                .cursor_pointer()
                .on_click(cx.listener(move |host, _, _, cx| {
                    cx.stop_propagation();
                    // As drawn: the press outside the card closed it already.
                    if shown {
                        close(host, cx);
                    } else {
                        open(host, key.clone(), agent.clone(), cx);
                    }
                }))
        })
        .child(child)
        .children(card)
        .into_any_element()
}

/// The open card, beside its trigger's top right corner.
fn floating<T: CardHost>(
    host: &T,
    agent_id: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    let body = super::agent_card_view::card_body(host, agent_id, window, cx);
    let card = div()
        .id("agent-card")
        .debug_selector(|| "agent-card".into())
        .occlude()
        .on_hover(cx.listener(|host, hovered: &bool, _, cx| {
            if *hovered {
                host.cards().leave = None;
            } else {
                leave_soon(host, cx);
            }
        }))
        .on_mouse_down_out(cx.listener(|host, _, _, cx| {
            close(host, cx);
        }))
        .child(body)
        .child(crate::ui::occlusion::marker());
    div()
        .absolute()
        .top_0()
        .left(gpui::relative(1.0))
        .size_0()
        .child(
            deferred(
                anchored()
                    .offset(point(px(8.0), px(0.0)))
                    .snap_to_window_with_margin(px(8.0))
                    .child(card),
            )
            .priority(crate::ui::layer::POPUP),
        )
        .into_any_element()
}

/// The agent `agent_id`, as the sidebar read it.
pub(crate) fn agent_of(
    sidebar: &Entity<Sidebar>,
    agent_id: &str,
    cx: &App,
) -> Option<crate::data::agents::AgentRecord> {
    sidebar
        .read(cx)
        .agents()
        .iter()
        .find(|agent| agent.id == agent_id)
        .cloned()
}
