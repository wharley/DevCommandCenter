//! An agent at work in the room's own conversation, the way Tucano shows a
//! run (`RunSteps`, `StepRow`): one row per agent in the answers' flat look
//! — the mascot at work, the bold name and the time since the turn started
//! — with what it does now on a soft pulse ("Lendo src/main.rs", else
//! "Pensando…", else "Lendo o pedido…"), the steps so far and the answer as
//! it streams, "Parar" and "Abrir conversa" beside the name. Once the turn
//! ends the row leaves and the answer takes its place with its steps
//! folded; a turn that stopped says so and offers "Tentar de novo".
//! Drawn in the list's tail (`room_items`), so a stream lays out only it.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dcc_core::domain::team_room_live::{
    Doing, LiveStep, LiveTurn, StepState, StoppedTurn, TurnNow,
};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use super::room_live_text::{LiveText, clock_text};
use crate::agent::{AgentAvatarView, Mood, MoodAvatarView};
use crate::data::agent_mentions::MentionNames;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::markdown::{MarkdownStyle, render_markdown};
use crate::theme::ActiveTheme;
use crate::ui::pulse::Rate;

use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, pulse_loop, pulse_spinner};

/// The steps listed while it works; the earlier ones are counted.
const STEPS_SHOWN: usize = 6;
/// With no answer yet after this long, the label says it takes a while.
const LONG_SECONDS: i64 = 60;
/// A stop older than this no longer asks to be tried again.
const STOPPED_HOURS: i64 = 24;

/// What a live row draws, read from the feeds before drawing; the turn is
/// shared with its feed, not copied each frame.
struct AtWork {
    agent: AgentRecord,
    session_id: String,
    now: Arc<TurnNow>,
}

impl TeamRoom {
    /// The agents at work in this room's conversations, one row each, and
    /// who they are (their "está trabalhando…" gives way).
    pub(super) fn live_turn_rows(
        &self,
        agents: &[AgentRecord],
        names: &MentionNames,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Vec<String>) {
        let mut work: Vec<AtWork> = Vec::new();
        for feed in self.live_feeds(cx) {
            let Some(agent) = agents.iter().find(|agent| agent.id == feed.agent_id) else {
                continue;
            };
            if work.iter().any(|known| known.agent.id == agent.id) {
                continue;
            }
            work.push(AtWork {
                agent: agent.clone(),
                session_id: feed.session_id.clone(),
                now: feed.now.clone(),
            });
        }
        let ids = work.iter().map(|at| at.agent.id.clone()).collect();
        let rows = work
            .into_iter()
            .enumerate()
            .map(|(index, at)| self.live_turn_row(index, at, names, window, cx))
            .collect();
        (rows, ids)
    }

    fn live_turn_row(
        &self,
        index: usize,
        at: AtWork,
        names: &MentionNames,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let AtWork {
            agent,
            session_id,
            now: turn_now,
        } = at;
        let Some(turn) = turn_now.live() else {
            return div().into_any_element();
        };
        let now = chrono::Utc::now().timestamp_millis();
        let seconds = turn
            .started_at
            .as_deref()
            .and_then(crate::data::time::parse_ms)
            .map_or(0, |started| (now - started) / 1000);
        let label = self.doing_label(turn, seconds, cx);
        let text = self.streamed(index, &session_id, turn, names, window, cx);
        let stopping = self.live.stopping.contains(&session_id);
        let workspace = agent
            .sessions
            .iter()
            .find(|session| session.session_id == session_id)
            .map(|session| session.workspace_id.clone())
            .unwrap_or_default();
        let (stop, open) = (session_id.clone(), session_id.clone());
        let header = div()
            .flex()
            .items_baseline()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.foreground)
                    .child(agent.name.clone()),
            )
            .child(
                div()
                    .debug_selector(move || format!("room-live-clock-{index}"))
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(clock_text(seconds)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id(SharedString::from(format!("room-live-open-{index}")))
                    .debug_selector(move || format!("room-live-open-{index}"))
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .cursor_pointer()
                    .hover(|link| link.text_color(colors.foreground))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(RoomEvent::OpenSession {
                            task_id: workspace.clone(),
                            session_id: open.clone(),
                        })
                    }))
                    .child(cx.t("agents.room.open")),
            )
            .child(
                Button::new(SharedString::from(format!("room-live-stop-{index}")))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .disabled(stopping)
                    .on_click(cx.listener(move |room, _, _, cx| room.stop_turn(stop.clone(), cx)))
                    .child(cx.t(if stopping {
                        "agents.room.live.stopping"
                    } else {
                        "agents.room.live.stop"
                    })),
            );
        div()
            .debug_selector(move || format!("room-live-{index}"))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                MoodAvatarView::new(
                    SharedString::from(format!("room-live-face-{index}")),
                    &agent.avatar,
                    Mood::Working,
                    32.0,
                )
                .on_clock(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(header)
                    .child(shimmer(index, label, colors.muted_foreground))
                    .children(self.live_steps(index, &turn.steps, cx))
                    .children(text),
            )
            .into_any_element()
    }

    /// "Lendo src/main.rs" while a tool runs, else what the turn is at.
    pub(super) fn doing_label(
        &self,
        turn: &LiveTurn,
        seconds: i64,
        cx: &gpui::App,
    ) -> SharedString {
        if let Some(step) = turn.running_step() {
            return step_text(step, cx);
        }
        if turn.text.trim().is_empty() && seconds >= LONG_SECONDS {
            return cx.t("agents.room.live.long");
        }
        // The answer is coming in: it writes, it no longer thinks.
        if !turn.text.trim().is_empty() {
            return cx.t("agents.room.live.writing");
        }
        cx.t(match turn.doing {
            Doing::Reading => "agents.room.live.reading",
            _ => "agents.room.live.thinking",
        })
    }

    /// The answer so far, paced, its markdown parsed now and then.
    fn streamed(
        &self,
        index: usize,
        session_id: &str,
        turn: &LiveTurn,
        names: &MentionNames,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if turn.text.trim().is_empty() {
            return None;
        }
        let at_once = cx.reduce_motion() || !crate::data::foreground::is_foreground(cx);
        let blocks = {
            let mut texts = self.live.texts.borrow_mut();
            let live = texts
                .entry(session_id.to_string())
                .or_insert_with(|| LiveText::new(&turn.turn_id, &turn.text));
            if live.turn_id != turn.turn_id {
                *live = LiveText::new(&turn.turn_id, &turn.text);
            }
            live.frame(&turn.text, Instant::now(), at_once, names).0
        };
        Some(
            div()
                .debug_selector(move || format!("room-live-text-{index}"))
                .text_size(px(13.0))
                .text_color(cx.theme().colors.foreground)
                .child(render_markdown(
                    &blocks,
                    MarkdownStyle {
                        size: 13.0,
                        ..MarkdownStyle::reply()
                    },
                    &format!("room-live-md-{index}"),
                    window,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The last steps, open while it works ("+N passos" for the earlier).
    fn live_steps(&self, index: usize, steps: &[LiveStep], cx: &gpui::App) -> Option<AnyElement> {
        if steps.is_empty() {
            return None;
        }
        let colors = cx.theme().colors.clone();
        let earlier = steps.len().saturating_sub(STEPS_SHOWN);
        let rows = steps[earlier..].iter().enumerate().map(|(n, step)| {
            let mark = match step.state {
                StepState::Running => pulse_spinner(
                    SharedString::from(format!("room-live-step-spin-{index}-{n}")),
                    11.0,
                )
                .into_any_element(),
                StepState::Done => Icon::new("check")
                    .size(11.0)
                    .color(colors.muted_foreground)
                    .into_any_element(),
                StepState::Failed => Icon::new("x")
                    .size(11.0)
                    .color(colors.destructive)
                    .into_any_element(),
            };
            div()
                .debug_selector(move || format!("room-live-step-{index}-{n}"))
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w_0()
                .text_size(px(11.5))
                .text_color(if step.state == StepState::Failed {
                    colors.destructive
                } else {
                    colors.muted_foreground
                })
                .child(div().flex_none().child(mark))
                .child(div().truncate().child(step_text(step, cx)))
                .into_any_element()
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(earlier > 0, |list| {
                    list.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(colors.muted_foreground)
                            .child(
                                cx.t_with("agents.room.steps.more", &Args::count(earlier as i64)),
                            ),
                    )
                })
                .children(rows)
                .into_any_element(),
        )
    }

    /// "Dev parou antes de terminar" with why, and "Tentar de novo" when
    /// the person sent or passed what it was doing; agents at work now show
    /// their live row instead.
    pub(super) fn stopped_rows(
        &self,
        agents: &[AgentRecord],
        working: &[String],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let now = chrono::Utc::now().timestamp_millis();
        let stopped: Vec<(AgentRecord, String, StoppedTurn)> = self
            .feeds
            .iter()
            .filter(|feed| self.shows(&feed.agent_id) && !working.contains(&feed.agent_id))
            .filter_map(|feed| {
                let turn = feed.now.stopped()?;
                let recent = turn
                    .ended_at
                    .as_deref()
                    .and_then(crate::data::time::parse_ms)
                    .is_some_and(|ended| now - ended < STOPPED_HOURS * 3_600_000);
                if !recent || self.live.retried.contains(&turn.turn_id) {
                    return None;
                }
                let agent = agents.iter().find(|agent| agent.id == feed.agent_id)?;
                Some((agent.clone(), feed.session_id.clone(), turn.clone()))
            })
            .collect();
        stopped
            .into_iter()
            .enumerate()
            .map(|(index, (agent, session, turn))| {
                self.stopped_row(index, agent, session, turn, cx)
            })
            .collect()
    }

    fn stopped_row(
        &self,
        index: usize,
        agent: AgentRecord,
        session_id: String,
        turn: StoppedTurn,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let said = cx.t_with(
            "agents.room.live.stopped",
            &Args::default().with("agent", agent.name.clone()),
        );
        let why = turn
            .reason
            .as_deref()
            .map(|reason| super::room_failure::reason_line(reason, cx));
        let text: SharedString = match why {
            Some(why) if !why.is_empty() => format!("{said} · {why}").into(),
            _ => said,
        };
        let face = AgentAvatarView::new(
            SharedString::from(format!("room-stopped-face-{index}")),
            &agent.avatar,
            None,
            22.0,
        );
        let retry = turn.retry.clone().map(|prompt| {
            let turn_id = turn.turn_id.clone();
            Button::new(SharedString::from(format!("room-retry-{index}")))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .on_click(cx.listener(move |room, _, _, cx| {
                    room.retry_turn(
                        agent.clone(),
                        session_id.clone(),
                        turn_id.clone(),
                        prompt.clone(),
                        cx,
                    )
                }))
                .child(cx.t("agents.room.live.retry"))
        });
        div()
            .debug_selector(move || format!("room-stopped-{index}"))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(face)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(colors.muted_foreground)
                    .child(text),
            )
            .children(retry)
            .into_any_element()
    }
}

/// "Lendo src/main.rs", "Rodou cargo test": the verb the task conversation
/// uses, live or done, and what it acted on.
fn step_text(step: &LiveStep, cx: &gpui::App) -> SharedString {
    let tense = if step.state == StepState::Running {
        "live"
    } else {
        "done"
    };
    let verb = cx.t(&format!(
        "conversation.toolCall.verbs.{}.{tense}",
        step.kind.key()
    ));
    match step.target.as_deref().map(str::trim) {
        Some(target) if !target.is_empty() => format!("{verb} {target}").into(),
        _ => verb,
    }
}

/// The label breathing softly on the shared clock; still under reduced
/// motion or with DCC in the back.
fn shimmer(index: usize, label: SharedString, color: gpui::Hsla) -> AnyElement {
    div()
        .debug_selector(move || format!("room-live-doing-{index}"))
        .text_size(px(12.0))
        .child(pulse_loop(
            SharedString::from(format!("room-live-doing-{index}")),
            Duration::from_millis(1600),
            Rate::Half,
            move |phase| {
                let wave = 0.5 + 0.5 * (phase * TAU).cos();
                div()
                    .truncate()
                    .text_color(color.opacity(0.55 + 0.45 * wave))
                    .child(label)
            },
        ))
        .into_any_element()
}
