//! What is going on now, under the room's conversation, the way Slack shows
//! who is typing: the tasks this room's agents opened that still run ("Dev ·
//! “Investigar…” rodando há 12 min"), and every agent at work in a task of
//! the room's project however it got there — a trigger included ("Revisor ·
//! Gatilho · CI falhou · trabalhando em “Código de rastreio” há 1 min").
//! Each works until the turn ends, a click opening it; the result card
//! takes its place.

use gpui::{AnyElement, Context};

use super::room::{RoomEvent, TeamRoom};
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
use crate::data::room_of::room_for;
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::{AgentAtWork, RunningTask};

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
                Some(self.live_row(
                    format!("room-running-{index}"),
                    agent,
                    None,
                    text,
                    move |_, cx| cx.emit(RoomEvent::OpenTask(id.clone())),
                    cx,
                ))
            })
            .collect()
    }
}

impl TeamRoom {
    /// The agents at work in a task of this room's project.
    fn at_work_here(&self, cx: &gpui::App) -> Vec<AgentAtWork> {
        let opened: Vec<String> = self
            .running_here(cx)
            .into_iter()
            .map(|task| task.task_id)
            .collect();
        self.sidebar
            .read(cx)
            .agents_at_work()
            .into_iter()
            .filter(|work| {
                self.shows(&work.agent_id)
                    && !opened.contains(&work.task_id)
                    && room_for(Some(work.project_id.clone()), &self.rooms) == self.room
            })
            .collect()
    }

    pub(super) fn at_work_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let now = chrono::Utc::now().timestamp_millis();
        let runs = cx
            .try_global::<AgentTriggers>()
            .map(|triggers| triggers.runs().to_vec())
            .unwrap_or_default();
        self.at_work_here(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(index, work)| {
                let agent = agents.iter().find(|agent| agent.id == work.agent_id)?;
                let time = elapsed(work.started_at.as_deref(), now).unwrap_or_default();
                let started = work
                    .started_at
                    .as_deref()
                    .and_then(crate::data::time::parse_ms)
                    .unwrap_or(now);
                let chip =
                    crate::data::room_inbox::woken_by(&runs, &agent.id, &work.task_id, started)
                        .map(|trigger| cx.t(&format!("agents.room.card.trigger.{}", trigger.id())));
                let text = cx.t_with(
                    if time.is_empty() {
                        "agents.room.atWorkNow"
                    } else {
                        "agents.room.atWork"
                    },
                    &Args::default()
                        .with("agent", agent.name.clone())
                        .with("task", work.task_name.clone())
                        .with("time", time),
                );
                let (task, session) = (work.task_id.clone(), work.session_id.clone());
                Some(self.live_row(
                    format!("room-at-work-{index}"),
                    agent,
                    chip,
                    text,
                    move |_, cx| {
                        cx.emit(RoomEvent::OpenSession {
                            task_id: task.clone(),
                            session_id: session.clone(),
                        })
                    },
                    cx,
                ))
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
