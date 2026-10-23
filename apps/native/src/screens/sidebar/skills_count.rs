//! The badge on the Skills tool: how much context the open task's checkout
//! gives its agents (`skills_detect_context`), counted off the UI thread when
//! the task changes and every half minute.

use gpui::Context;

use super::Sidebar;
use dcc_infra::skills_context::{detect_context_sources, total_skill_context_count};

impl Sidebar {
    pub(super) fn refresh_skill_count(&mut self, cx: &mut Context<Self>) {
        let root = self
            .state
            .selected_task_id
            .as_deref()
            .and_then(|id| self.state.task(id))
            .map(|task| {
                task.worktree_path
                    .as_deref()
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .unwrap_or(task.root_path.trim())
                    .to_string()
            });
        let Some(root) = root.filter(|root| !root.is_empty()) else {
            if self.skill_count != 0 {
                self.skill_count = 0;
                cx.notify();
            }
            return;
        };
        cx.spawn(async move |this, cx| {
            let count = cx
                .background_executor()
                .spawn(async move {
                    total_skill_context_count(&detect_context_sources(&root, Some(&root)))
                })
                .await;
            let _ = this.update(cx, |sidebar, cx| {
                if sidebar.skill_count != count {
                    sidebar.skill_count = count;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
