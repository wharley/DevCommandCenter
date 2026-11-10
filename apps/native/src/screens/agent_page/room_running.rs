//! The tasks this room's agents opened that are still running, under the
//! conversation: "Dev · “Investigar…” rodando há 12 min", working until the
//! task ends its turn, a click opening it — so an agent's "Iniciei a
//! tarefa" is not read as the work being done.

use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

use super::room::{RoomEvent, TeamRoom};
use crate::agent::AgentAvatarView;
use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::RunningTask;
use crate::theme::ActiveTheme;
use crate::ui::spinner;

/// "3 min", "1 h 5 min"; under a minute, "agora".
pub(crate) fn elapsed(started_at: Option<&str>, now_ms: i64) -> Option<String> {
    let started = crate::data::time::parse_ms(started_at?)?;
    let minutes = (now_ms - started).max(0) / 60_000;
    Some(match minutes {
        0 => String::new(),
        1..=59 => format!("{minutes} min"),
        _ => format!("{} h {} min", minutes / 60, minutes % 60),
    })
}

impl TeamRoom {
    /// The running tasks opened from this room's conversations.
    fn running_here(&self, cx: &gpui::App) -> Vec<RunningTask> {
        self.sidebar
            .read(cx)
            .running_agent_tasks()
            .into_iter()
            .filter(|task| {
                self.shows(&task.agent_id)
                    && self
                        .feeds
                        .iter()
                        .any(|feed| feed.session_id == task.session_id)
            })
            .collect()
    }

    pub(super) fn running_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors.clone();
        let now = chrono::Utc::now().timestamp_millis();
        self.running_here(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(index, task)| {
                let agent = agents.iter().find(|agent| agent.id == task.agent_id)?;
                let time = elapsed(task.started_at.as_deref(), now).unwrap_or_default();
                let text = cx.t_with(
                    if time.is_empty() {
                        "agents.room.taskRunningNow"
                    } else {
                        "agents.room.taskRunning"
                    },
                    &Args::default()
                        .with("agent", agent.name.clone())
                        .with("task", task.name.clone())
                        .with("time", time),
                );
                let id = task.task_id.clone();
                Some(
                    div()
                        .id(SharedString::from(format!("room-running-{index}")))
                        .debug_selector(move || format!("room-running-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(12.0))
                        .border_1()
                        .border_color(colors.border.opacity(0.7))
                        .bg(colors.card)
                        .cursor_pointer()
                        .hover(|row| row.bg(colors.accent.opacity(0.4)))
                        .on_click(
                            cx.listener(move |_, _, _, cx| {
                                cx.emit(RoomEvent::OpenTask(id.clone()))
                            }),
                        )
                        .child(AgentAvatarView::new(
                            SharedString::from(format!("room-running-face-{index}")),
                            &agent.avatar,
                            None,
                            22.0,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(colors.foreground)
                                .child(text),
                        )
                        .child(spinner(
                            SharedString::from(format!("room-running-spin-{index}")),
                            14.0,
                        ))
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_time_running_reads_short() {
        let start = "2026-11-10T12:00:00Z";
        let at = |minutes: i64| crate::data::time::parse_ms(start).unwrap() + minutes * 60_000;
        assert_eq!(super::elapsed(Some(start), at(0)).as_deref(), Some(""));
        assert_eq!(
            super::elapsed(Some(start), at(12)).as_deref(),
            Some("12 min")
        );
        assert_eq!(
            super::elapsed(Some(start), at(65)).as_deref(),
            Some("1 h 5 min")
        );
        assert_eq!(super::elapsed(None, at(1)), None);
    }
}
