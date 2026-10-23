//! The rail's model: what it lists and in which order, as `sidebar.tsx`
//! builds `flatItems`. Pure, so every rule is tested without a window.

use std::collections::{HashMap, HashSet};

use dcc_core::domain::attention::{
    AgentActivityState, AttentionItem, BlockerFirstSeen, WorkspaceBlocker,
};

use crate::data::git_watch::{GitSnapshot, Watched};
use crate::data::loader::SidebarData;
use crate::data::rail::{self, FrozenOrder, TaskSectionId};
use crate::data::recap::{self, RailMeta, Recap, RecapInput};
use crate::data::seen::SeenResults;
use crate::data::tasks::TaskSummary;

pub const DAY_LABEL_HEIGHT: f32 = 26.0;
pub const ROW_HEIGHT: f32 = 54.0;
pub const HEADER_HEIGHT: f32 = 42.0;
pub const CHILD_TASK_HEIGHT: f32 = 26.0;
const GROUP_GAP: f32 = 10.0;
const EMPTY_GROUP_GAP: f32 = 8.0;
const BOTTOM_PADDING: f32 = 8.0;
/// "Needs you" shows this many before "Show N more".
pub const ATTENTION_PREVIEW_LIMIT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoldSection {
    /// "Em espera": paused or snoozed.
    Waiting,
    Completed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RailItem {
    DayLabel(TaskSectionId),
    Row {
        task_id: String,
        /// On hold and completed rows are two-line and quieter.
        compact: bool,
    },
    Header {
        section: HoldSection,
        count: usize,
        open: bool,
    },
    /// A task an agent proposed or started, under the task it came from.
    ChildTask {
        task_id: String,
        child_id: String,
        /// The guide line stops at the last one.
        last: bool,
    },
    Gap(f32),
    BottomPadding,
}

impl RailItem {
    pub fn height(&self) -> f32 {
        match self {
            RailItem::DayLabel(_) => DAY_LABEL_HEIGHT,
            RailItem::Row { .. } => ROW_HEIGHT,
            RailItem::Header { .. } => HEADER_HEIGHT,
            RailItem::ChildTask { .. } => CHILD_TASK_HEIGHT,
            RailItem::Gap(size) => *size,
            RailItem::BottomPadding => BOTTOM_PADDING,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SidebarState {
    pub data: SidebarData,
    pub seen: SeenResults,
    pub selected_task_id: Option<String>,
    pub waiting_open: bool,
    pub completed_open: bool,
    /// Task order held while the pointer is over the list.
    pub frozen_order: Option<FrozenOrder>,
    /// "Needs you" held while the pointer is over the sidebar.
    pub frozen_attention: Option<Vec<AttentionItem>>,
    pub show_all_attention: bool,
    pub last_running_jump: Option<String>,
    pub now_ms: i64,
    /// Git and PR state of the open tasks (`git_watch`).
    pub git: HashMap<String, GitSnapshot>,
    pub blockers: Vec<WorkspaceBlocker>,
    pub first_seen: BlockerFirstSeen,
}

/// What a row shows from git: its live branch, the next-action recap and the
/// PR/diff chips (`useWorkspaceRailRecap`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowGit {
    pub current_branch: String,
    pub recap: Option<Recap>,
    pub meta: Option<RailMeta>,
}

impl SidebarState {
    pub fn task(&self, id: &str) -> Option<&TaskSummary> {
        self.data.tasks.iter().find(|task| task.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.data.tasks.is_empty()
    }

    /// `attentionItems`, live.
    pub fn attention(&self) -> Vec<AttentionItem> {
        rail::needs_you(
            &self.data.tasks,
            &self.data.activities,
            &self.blockers,
            &|id, at| self.seen.is_unread(id, at),
            self.now_ms,
        )
    }

    /// The open tasks whose checkout the rail follows.
    pub fn watched(&self) -> Vec<Watched> {
        self.data
            .tasks
            .iter()
            .filter(|task| task.status.is_selectable())
            .map(|task| Watched {
                id: task.id.clone(),
                root: task
                    .worktree_path
                    .clone()
                    .unwrap_or_else(|| task.root_path.clone()),
                branch: task.branch.clone(),
                finished_turn: self
                    .data
                    .activities
                    .get(&task.id)
                    .is_some_and(|activity| !activity.is_turn_open()),
            })
            .collect()
    }

    /// Delegated edits waiting on the person for a task.
    pub fn pending_reviews(&self, task_id: &str) -> usize {
        self.data.review_pending.get(task_id).copied().unwrap_or(0)
    }

    /// `useWorkspaceRailRecap`: nothing until git and the PR have answered
    /// (unless a turn is open), and, for a finished clean task without a PR,
    /// until the branch diff has too.
    pub fn row_git(&self, task: &TaskSummary) -> RowGit {
        let fallback = RowGit {
            current_branch: task.branch.clone(),
            ..RowGit::default()
        };
        if !task.status.is_selectable() {
            return fallback;
        }
        let Some(snapshot) = self.git.get(&task.id) else {
            return fallback;
        };
        let current_branch = snapshot
            .current_branch()
            .unwrap_or(&task.branch)
            .to_string();
        let meta = recap::build_meta(&current_branch, snapshot.git.as_ref(), snapshot.pr.as_ref());
        let activity = self.data.activities.get(&task.id);
        let turn_open = activity.is_some_and(|activity| activity.is_turn_open());
        let pending = snapshot.git_at.is_none() || snapshot.pr_at.is_none();
        let needs_diff = activity.is_some()
            && !turn_open
            && snapshot.is_clean()
            && snapshot.pr.as_ref().is_some_and(|pr| pr.number.is_none());
        let recap = if (!turn_open && pending) || (needs_diff && snapshot.diff_at.is_none()) {
            None
        } else {
            recap::build_recap(&RecapInput {
                branch: &current_branch,
                turn_running: activity
                    .is_some_and(|activity| activity.activity.state == AgentActivityState::Active),
                has_activity: activity.is_some(),
                git: snapshot.git.as_ref(),
                pr: snapshot.pr.as_ref(),
                committed_vs_base: snapshot.committed_vs_base.unwrap_or(0),
                pending_delegations: self.pending_reviews(&task.id),
            })
        };
        RowGit {
            current_branch,
            recap,
            meta,
        }
    }

    /// What "Needs you" shows: frozen while hovered, then cut to the preview.
    pub fn shown_attention(&self) -> (Vec<AttentionItem>, usize) {
        let all = self
            .frozen_attention
            .clone()
            .unwrap_or_else(|| self.attention());
        let total = all.len();
        let visible = if self.show_all_attention {
            all
        } else {
            all.into_iter().take(ATTENTION_PREVIEW_LIMIT).collect()
        };
        let hidden = total - visible.len();
        (visible, hidden)
    }

    /// Finished results the person has not opened: the row reads "New result".
    pub fn unseen_result_ids(&self) -> HashSet<String> {
        use dcc_core::domain::attention::AttentionReason::{Aborted, Completed};
        self.attention()
            .into_iter()
            .filter(|item| matches!(item.reason, Completed | Aborted))
            .map(|item| item.workspace_id)
            .collect()
    }

    /// Agents at work; one waiting on the person is under "Needs you".
    pub fn running_ids(&self) -> HashSet<String> {
        self.data
            .tasks
            .iter()
            .filter(|task| task.status.is_selectable())
            .filter(|task| {
                self.data
                    .activities
                    .get(&task.id)
                    .is_some_and(|activity| activity.is_running())
            })
            .map(|task| task.id.clone())
            .collect()
    }

    pub fn task_sections(&self) -> Vec<rail::TaskSection> {
        let split = rail::split(&self.data.tasks, self.now_ms);
        rail::task_list(
            &split.active,
            &self.data.activities,
            self.now_ms,
            self.frozen_order.as_ref(),
        )
    }

    /// `flatItems`: day sections, then "On hold" and "Completed" when they
    /// have tasks, each header followed by its rows while open.
    pub fn items(&self) -> Vec<RailItem> {
        let split = rail::split(&self.data.tasks, self.now_ms);
        let mut items = Vec::new();
        let sections = self.task_sections();
        for section in &sections {
            if !items.is_empty() {
                items.push(RailItem::Gap(GROUP_GAP));
            }
            items.push(RailItem::DayLabel(section.id));
            for task in &section.tasks {
                items.push(RailItem::Row {
                    task_id: task.id.clone(),
                    compact: false,
                });
                self.push_child_tasks(&mut items, &task.id);
            }
        }
        let mut previous_has_rows = !sections.is_empty();
        for (section, rows, open) in [
            (HoldSection::Waiting, &split.waiting, self.waiting_open),
            (
                HoldSection::Completed,
                &split.completed,
                self.completed_open,
            ),
        ] {
            if rows.is_empty() {
                continue;
            }
            items.push(RailItem::Gap(if previous_has_rows {
                GROUP_GAP
            } else {
                EMPTY_GROUP_GAP
            }));
            items.push(RailItem::Header {
                section,
                count: rows.len(),
                open,
            });
            if open {
                for task in rows {
                    items.push(RailItem::Row {
                        task_id: task.id.clone(),
                        compact: true,
                    });
                    // A finished task's phases are over; only live work nests.
                    if section != HoldSection::Completed {
                        self.push_child_tasks(&mut items, &task.id);
                    }
                }
            }
            previous_has_rows = true;
        }
        items.push(RailItem::BottomPadding);
        items
    }

    fn push_child_tasks(&self, items: &mut Vec<RailItem>, task_id: &str) {
        let Some(children) = self.data.child_tasks.get(task_id) else {
            return;
        };
        for (index, child) in children.iter().enumerate() {
            items.push(RailItem::ChildTask {
                task_id: task_id.to_string(),
                child_id: child.id.clone(),
                last: index + 1 == children.len(),
            });
        }
    }

    pub fn toggle_section(&mut self, section: HoldSection) {
        match section {
            HoldSection::Waiting => self.waiting_open = !self.waiting_open,
            HoldSection::Completed => self.completed_open = !self.completed_open,
        }
    }

    /// Opening a task marks its latest result as seen and opens the section
    /// it sits in, like the desktop's auto-expand.
    pub fn select(&mut self, task_id: &str) {
        self.selected_task_id = Some(task_id.to_string());
        if let Some(activity) = self.data.activities.get(task_id)
            && matches!(
                activity.activity.state,
                dcc_core::domain::attention::AgentActivityState::Completed
                    | dcc_core::domain::attention::AgentActivityState::Aborted
            )
            && let Some(completed_at) = activity.activity.completed_at.clone()
        {
            self.seen.mark_seen(task_id, &completed_at);
        }
        let split = rail::split(&self.data.tasks, self.now_ms);
        if split.waiting.iter().any(|task| task.id == task_id) {
            self.waiting_open = true;
        }
        if split.completed.iter().any(|task| task.id == task_id) {
            self.completed_open = true;
        }
    }

    /// "N running": the next running task in list order.
    pub fn next_running(&mut self) -> Option<String> {
        let items = self.items();
        let ordered: Vec<&str> = items
            .iter()
            .filter_map(|item| match item {
                RailItem::Row { task_id, .. } => Some(task_id.as_str()),
                _ => None,
            })
            .collect();
        let running = self.running_ids();
        let next = rail::next_running_task_id(
            &ordered,
            &|id| running.contains(id),
            self.last_running_jump.as_deref(),
        )?
        .to_string();
        self.last_running_jump = Some(next.clone());
        Some(next)
    }
}

#[cfg(test)]
mod tests;
