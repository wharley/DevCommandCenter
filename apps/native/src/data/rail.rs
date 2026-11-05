//! The rail's lists, one to one with `workspace-rail-projection.ts` and
//! `attentionWorkspaceItems`: active tasks split by day, "On hold",
//! "Completed" and "Needs you".

use std::collections::HashMap;

use dcc_core::domain::attention::{
    AgentActivity, AttentionItem, AttentionWorkspace, WorkspaceBlocker, attention_items,
};

use super::activity::TaskActivity;
use super::tasks::{TaskStatus, TaskSummary};
use super::time::{parse_ms, start_of_local_day};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Snoozed until a future instant: kept in "On hold" until it passes.
pub fn is_snoozed(task: &TaskSummary, now_ms: i64) -> bool {
    task.snoozed_until
        .as_deref()
        .and_then(parse_ms)
        .is_some_and(|at| at > now_ms)
}

pub struct RailSplit {
    /// Neither on hold nor completed: the single list.
    pub active: Vec<TaskSummary>,
    pub waiting: Vec<TaskSummary>,
    pub completed: Vec<TaskSummary>,
}

pub fn split(tasks: &[TaskSummary], now_ms: i64) -> RailSplit {
    let waiting = tasks
        .iter()
        .filter(|task| {
            task.status == TaskStatus::Archived
                || (task.status != TaskStatus::Completed && is_snoozed(task, now_ms))
        })
        .cloned()
        .collect();
    let completed = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .cloned()
        .collect();
    let active = tasks
        .iter()
        .filter(|task| task.status.is_selectable() && !is_snoozed(task, now_ms))
        .cloned()
        .collect();
    RailSplit {
        active,
        waiting,
        completed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TaskSectionId {
    Pinned,
    Today,
    Yesterday,
    Week,
    Older,
}

impl TaskSectionId {
    const ORDER: [Self; 5] = [
        Self::Pinned,
        Self::Today,
        Self::Yesterday,
        Self::Week,
        Self::Older,
    ];

    /// `sidebar.taskSection.*`
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Pinned => "sidebar.taskSection.pinned",
            Self::Today => "sidebar.taskSection.today",
            Self::Yesterday => "sidebar.taskSection.yesterday",
            Self::Week => "sidebar.taskSection.week",
            Self::Older => "sidebar.taskSection.older",
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskSection {
    pub id: TaskSectionId,
    pub tasks: Vec<TaskSummary>,
}

/// Where each task sat when the pointer entered the list.
pub type FrozenOrder = Vec<(String, TaskSectionId)>;

/// `taskInteractionMs`: the person's newest turn, or the task's creation.
fn interaction_ms(task: &TaskSummary, last_interaction_at: Option<&str>) -> i64 {
    [last_interaction_at, Some(task.created_at.as_str())]
        .into_iter()
        .flatten()
        .filter_map(parse_ms)
        .max()
        .unwrap_or(i64::MIN)
}

/// `taskListSectionFor`
fn section_for(task: &TaskSummary, interaction_ms: i64, now_ms: i64) -> TaskSectionId {
    if task.pinned_at.is_some() {
        return TaskSectionId::Pinned;
    }
    let today = start_of_local_day(now_ms);
    if interaction_ms >= today {
        return TaskSectionId::Today;
    }
    if interaction_ms >= start_of_local_day(today - 1) {
        return TaskSectionId::Yesterday;
    }
    if interaction_ms >= start_of_local_day(today - 6 * DAY_MS) {
        return TaskSectionId::Week;
    }
    TaskSectionId::Older
}

/// `projectWorkspaceTaskList`: the running ones first, then newest
/// interaction, split by day, pinned on top in pin order. With `frozen`, known tasks keep their place (nothing
/// moves under the pointer), new ones go on top of their section, and a pin
/// change, being the person's own click, applies at once.
pub fn task_list(
    active: &[TaskSummary],
    activities: &HashMap<String, TaskActivity>,
    now_ms: i64,
    frozen: Option<&FrozenOrder>,
) -> Vec<TaskSection> {
    let last = |task: &TaskSummary| {
        activities
            .get(&task.id)
            .and_then(|activity| activity.last_interaction_at.as_deref())
    };
    let mut ranked: Vec<(&TaskSummary, i64)> = active
        .iter()
        .map(|task| (task, interaction_ms(task, last(task))))
        .collect();
    let running = |task: &TaskSummary| {
        activities
            .get(&task.id)
            .is_some_and(|activity| activity.is_running())
    };
    ranked.sort_by(|(a, a_ms), (b, b_ms)| match (&a.pinned_at, &b.pinned_at) {
        (Some(a_pin), Some(b_pin)) => a_pin.cmp(b_pin).then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => running(b)
            .cmp(&running(a))
            .then_with(|| b_ms.cmp(a_ms))
            .then_with(|| a.id.cmp(&b.id)),
    });
    let frozen_index: HashMap<&str, (usize, TaskSectionId)> = frozen
        .map(|order| {
            order
                .iter()
                .enumerate()
                .map(|(index, (id, section))| (id.as_str(), (index, *section)))
                .collect()
        })
        .unwrap_or_default();
    let total = ranked.len() as i64;
    let mut by_section: HashMap<TaskSectionId, Vec<(&TaskSummary, i64)>> = HashMap::new();
    for (position, (task, ms)) in ranked.into_iter().enumerate() {
        let live = section_for(task, ms, now_ms);
        let known = frozen_index.get(task.id.as_str());
        let keeps_place = known.is_some_and(|(_, section)| {
            (*section == TaskSectionId::Pinned) == (live == TaskSectionId::Pinned)
        });
        let (section, rank) = match known {
            Some((index, section)) if keeps_place => (*section, *index as i64),
            _ => (live, position as i64 - total),
        };
        by_section.entry(section).or_default().push((task, rank));
    }
    TaskSectionId::ORDER
        .into_iter()
        .filter_map(|id| {
            let mut entries = by_section.remove(&id)?;
            entries.sort_by_key(|(_, rank)| *rank);
            Some(TaskSection {
                id,
                tasks: entries.into_iter().map(|(task, _)| task.clone()).collect(),
            })
        })
        .collect()
}

/// The order the list shows now, to freeze while the pointer is over it.
pub fn freeze(sections: &[TaskSection]) -> FrozenOrder {
    sections
        .iter()
        .flat_map(|section| {
            section
                .tasks
                .iter()
                .map(move |task| (task.id.clone(), section.id))
        })
        .collect()
}

/// `nextRunningTaskId`: the running task after the last one jumped to,
/// wrapping around.
pub fn next_running_task_id<'a>(
    ordered_ids: &[&'a str],
    running: &dyn Fn(&str) -> bool,
    last: Option<&str>,
) -> Option<&'a str> {
    let last_index = last.and_then(|id| ordered_ids.iter().position(|candidate| *candidate == id));
    let running_ids: Vec<(usize, &'a str)> = ordered_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| running(id))
        .map(|(index, id)| (index, *id))
        .collect();
    running_ids
        .iter()
        .find(|(index, _)| last_index.is_none_or(|last| *index > last))
        .or_else(|| running_ids.first())
        .map(|(_, id)| *id)
}

/// "Needs you": the shared rule, minus finished results the person has
/// already opened (`is_unread`).
pub fn needs_you(
    tasks: &[TaskSummary],
    activities: &HashMap<String, TaskActivity>,
    blockers: &[WorkspaceBlocker],
    is_unread: &dyn Fn(&str, Option<&str>) -> bool,
    now_ms: i64,
) -> Vec<AttentionItem> {
    let workspaces: Vec<AttentionWorkspace> = tasks
        .iter()
        .map(|task| AttentionWorkspace {
            id: task.id.clone(),
            status: task.status.as_str().to_string(),
            snoozed_until: task.snoozed_until.clone(),
            created_at: Some(task.created_at.clone()),
            updated_at: Some(task.updated_at.clone()),
        })
        .collect();
    let agent: HashMap<String, AgentActivity> = activities
        .iter()
        .map(|(id, activity)| (id.clone(), activity.activity.clone()))
        .collect();
    attention_items(&workspaces, &agent, blockers, now_ms)
        .into_iter()
        .filter(|item| {
            use dcc_core::domain::attention::AttentionReason::{Aborted, Completed};
            !matches!(item.reason, Completed | Aborted)
                || is_unread(&item.workspace_id, item.completed_at.as_deref())
        })
        .collect()
}

#[cfg(test)]
mod tests;
