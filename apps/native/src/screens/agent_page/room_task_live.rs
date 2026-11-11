//! A task at work under the room's conversation, live, as the room's own
//! turns are (`room_live_turn`): the agent's face at work, "Dev ·
//! “Corrigir domínio”", a clock since the task's turn started, what it does
//! now on the same pulse ("Editando checkout.tsx", "Pensando…") and its last
//! steps, "Abrir tarefa" on the right. The row follows only the task's own
//! conversation (the one the agent opened, or the one an agent works in),
//! read for its open turn by the room's live loop (`room_live_poll`); its
//! answer comes later, in the result card. Once that turn ends the row
//! leaves.

use gpui::{AnyElement, App, Context, FontWeight, SharedString, div, prelude::*, px};

use super::room::TeamRoom;
use super::room_live_text::clock_text;
use super::room_live_turn::{shimmer, step_list};
use crate::agent::{Mood, MoodAvatarView};
use crate::data::agents::AgentRecord;
use crate::data::team_room::HomeFeed;
use crate::data::team_room_poll::LiveProbe;
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;

/// The steps listed under a task at work; the earlier ones are counted.
const TASK_STEPS_SHOWN: usize = 3;

/// One task row to draw.
pub(super) struct TaskRun<'a> {
    /// Names the row for tests ("room-running-0").
    pub key: String,
    pub agent: &'a AgentRecord,
    pub task_name: String,
    /// The conversation followed, once known.
    pub session_id: Option<String>,
    /// When the task's turn started, as the sidebar read it: the clock's
    /// start until the turn itself is read.
    pub started_at: Option<String>,
    /// A trigger's chip ("Gatilho · CI falhou").
    pub chip: Option<SharedString>,
}

impl TeamRoom {
    /// The conversations the task rows follow, with their agent: each
    /// running task's own and each agent's at work in a task here.
    pub(super) fn followed_tasks(&self, cx: &App) -> Vec<(String, String)> {
        self.running_here(cx)
            .into_iter()
            .filter_map(|task| Some((task.agent_id, task.task_session_id?)))
            .chain(
                self.at_work_here(cx)
                    .into_iter()
                    .map(|work| (work.agent_id, work.session_id)),
            )
            .collect()
    }

    /// What this beat reads for the task rows: what is no longer followed
    /// is dropped, what is new starts from its first event.
    pub(super) fn task_probes(&mut self, cx: &App) -> Vec<LiveProbe> {
        let followed = self.followed_tasks(cx);
        self.live.tasks.retain(|feed| {
            followed
                .iter()
                .any(|(_, session)| *session == feed.session_id)
        });
        for (agent, session) in &followed {
            if !self
                .live
                .tasks
                .iter()
                .any(|feed| feed.session_id == *session)
            {
                self.live.tasks.push(HomeFeed::new(agent, session));
            }
        }
        self.live.tasks.iter().map(HomeFeed::turn_probe).collect()
    }

    /// The row, or none once its turn was read ended.
    pub(super) fn task_row(
        &self,
        run: TaskRun,
        open: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let colors = cx.theme().colors.clone();
        let feed = run.session_id.as_deref().and_then(|session| {
            self.live
                .tasks
                .iter()
                .find(|feed| feed.session_id == session && feed.last_sequence() > 0)
        });
        let turn = match feed {
            Some(feed) => Some(feed.now.live()?.clone()),
            None => None,
        };
        let started = turn
            .as_ref()
            .and_then(|turn| turn.started_at.as_deref())
            .or(run.started_at.as_deref())
            .and_then(crate::data::time::parse_ms);
        let seconds = started.map_or(0, |started| (wall_ms(cx) - started) / 1000);
        let clock = clock_text(seconds);
        let label = turn
            .as_ref()
            .map(|turn| self.doing_label(turn, seconds, cx));
        #[cfg(test)]
        self.live.drawn.borrow_mut().insert(
            run.key.clone(),
            (
                clock.clone(),
                label
                    .as_ref()
                    .map(|label| label.to_string())
                    .unwrap_or_default(),
            ),
        );
        let key = run.key;
        let title = cx.t_with(
            "agents.room.task.title",
            &Args::default()
                .with("agent", run.agent.name.clone())
                .with("task", run.task_name),
        );
        let chip = run.chip.map(|chip| {
            div()
                .flex_none()
                .px(px(6.0))
                .rounded(px(4.0))
                .bg(colors.accent)
                .text_size(px(10.0))
                .text_color(colors.muted_foreground)
                .child(chip)
        });
        let (clock_key, open_key) = (format!("{key}-clock"), format!("{key}-open"));
        let header = div()
            .flex()
            .items_baseline()
            .gap(px(8.0))
            .min_w_0()
            .children(chip)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(title),
            )
            .child(
                div()
                    .debug_selector(move || clock_key.clone())
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(colors.muted_foreground)
                    .child(clock),
            )
            .child(div().flex_1())
            .child(
                div()
                    .debug_selector(move || open_key.clone())
                    .flex_none()
                    .text_size(px(11.5))
                    .text_color(colors.muted_foreground)
                    .child(cx.t("agents.room.task.open")),
            );
        let steps = turn
            .as_ref()
            .and_then(|turn| step_list(&format!("{key}-step"), &turn.steps, TASK_STEPS_SHOWN, cx));
        let selector = key.clone();
        Some(
            div()
                .id(SharedString::from(key.clone()))
                .debug_selector(move || selector.clone())
                .flex()
                .items_start()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(8.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(colors.border.opacity(0.7))
                .bg(colors.card)
                .cursor_pointer()
                .hover(|row| row.bg(colors.accent.opacity(0.4)))
                .on_click(cx.listener(move |room, _, _, cx| open(room, cx)))
                .child(
                    MoodAvatarView::new(
                        SharedString::from(format!("{key}-face")),
                        &run.agent.avatar,
                        Mood::Working,
                        22.0,
                    )
                    .on_clock(),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(header)
                        .children(label.map(|label| {
                            shimmer(format!("{key}-doing"), label, colors.muted_foreground)
                        }))
                        .children(steps),
                )
                .into_any_element(),
        )
    }
}

/// The wall clock the task clocks read. Under test it moves with the
/// executor's clock, so a test can advance it.
fn wall_ms(cx: &App) -> i64 {
    #[cfg(test)]
    {
        use std::cell::Cell;
        use std::time::Instant;
        thread_local! {
            static ANCHOR: Cell<Option<(Instant, i64)>> = const { Cell::new(None) };
        }
        let now = cx.background_executor().now();
        let (at, ms) = ANCHOR.with(|anchor| {
            let set = anchor
                .get()
                .unwrap_or((now, chrono::Utc::now().timestamp_millis()));
            anchor.set(Some(set));
            set
        });
        return ms + now.saturating_duration_since(at).as_millis() as i64;
    }
    #[cfg(not(test))]
    {
        let _ = cx;
        chrono::Utc::now().timestamp_millis()
    }
}
