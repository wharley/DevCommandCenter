//! Skills over the window (`setIsSkillsOpen`): the sidebar's sparkles and
//! the palette open it on the open task's checkout; a change recounts the
//! sidebar's badge (`onSkillsChanged`).

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::Shell;
use crate::screens::skills::{Skills, SkillsEvent};

pub(super) type OpenSkills = (Entity<Skills>, Subscription);

impl Shell {
    pub(super) fn open_skills(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.skills.is_some() {
            return;
        }
        let conversation = self.conversation.read(cx);
        let root = conversation.workspace_path();
        let task = conversation.data().map(|data| data.task_id.clone());
        let skills = cx.new(|cx| Skills::new(root, task, window, cx));
        let events = cx.subscribe(&skills, |shell, _, event, cx| match event {
            SkillsEvent::Closed => {
                shell.skills = None;
                cx.notify();
            }
            SkillsEvent::Changed => shell
                .sidebar
                .update(cx, |sidebar, cx| sidebar.refresh_skill_count(cx)),
        });
        self.skills = Some((skills, events));
        cx.notify();
    }

    pub(super) fn skills_overlay(&self) -> Option<AnyElement> {
        self.skills
            .as_ref()
            .map(|(skills, _)| skills.clone().into_any_element())
    }
}
