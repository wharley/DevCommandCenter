//! What is going on now, under the room's conversation, the way Slack shows
//! who is typing: the tasks this room's agents opened that still run ("Dev ·
//! “Investigar…” 2:14 · Editando checkout.tsx"), and every agent at work in
//! a task of the room's project however it got there — a trigger included
//! ("Gatilho · CI falhou · Revisor · “Código de rastreio”").
//! Each shows live until the turn ends (`room_task_live`), a click opening
//! it; the result card takes its place.

use gpui::{AnyElement, Context};

use super::room::{RoomEvent, TeamRoom};
use super::room_task_live::TaskRun;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::agents::AgentRecord;
use crate::data::room_of::room_for;
use crate::i18n::Translate;
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
    /// The running tasks opened from this room's conversations, and in a
    /// project's room every task of the project an agent opened from any
    /// room — the project's work shows in its room, as Tucano posts a
    /// project's events in its channel.
    pub(super) fn running_here(&self, cx: &gpui::App) -> Vec<RunningTask> {
        self.sidebar
            .read(cx)
            .running_agent_tasks()
            .into_iter()
            .filter(|task| {
                let asked_here = self
                    .feeds
                    .iter()
                    .any(|feed| feed.session_id == task.session_id);
                let of_this_project = self.room.is_some()
                    && room_for(Some(task.project_id.clone()), &self.rooms) == self.room;
                self.shows(&task.agent_id) && (asked_here || of_this_project)
            })
            .collect()
    }

    pub(super) fn running_rows(
        &self,
        agents: &[AgentRecord],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.running_here(cx)
            .into_iter()
            .enumerate()
            .filter_map(|(index, task)| {
                let agent = agents.iter().find(|agent| agent.id == task.agent_id)?;
                let id = task.task_id.clone();
                self.task_row(
                    TaskRun {
                        key: format!("room-running-{index}"),
                        agent,
                        task_name: task.name,
                        session_id: task.task_session_id,
                        started_at: task.started_at,
                        chip: None,
                    },
                    move |_, cx| cx.emit(RoomEvent::OpenTask(id.clone())),
                    cx,
                )
            })
            .collect()
    }
}

impl TeamRoom {
    /// The agents at work in a task of this room's project.
    pub(super) fn at_work_here(&self, cx: &gpui::App) -> Vec<AgentAtWork> {
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
                let started = work
                    .started_at
                    .as_deref()
                    .and_then(crate::data::time::parse_ms)
                    .unwrap_or(now);
                let chip =
                    crate::data::room_inbox::woken_by(&runs, &agent.id, &work.task_id, started)
                        .map(|trigger| cx.t(&format!("agents.room.card.trigger.{}", trigger.id())));
                let (task, session) = (work.task_id.clone(), work.session_id.clone());
                self.task_row(
                    TaskRun {
                        key: format!("room-at-work-{index}"),
                        agent,
                        task_name: work.task_name,
                        session_id: Some(work.session_id),
                        started_at: work.started_at,
                        chip,
                    },
                    move |_, cx| {
                        cx.emit(RoomEvent::OpenSession {
                            task_id: task.clone(),
                            session_id: session.clone(),
                        })
                    },
                    cx,
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
