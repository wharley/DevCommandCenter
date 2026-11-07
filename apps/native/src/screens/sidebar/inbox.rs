//! The team room's inbox as the sidebar knows it (`data::room_inbox`): the
//! cards from the agents it reads, what "Agentes" counts, and marking cards
//! read.

use gpui::Context;

use super::Sidebar;
use crate::data::agent_triggers::Run;
use crate::data::room_inbox::{CardKind, InboxCard, InboxSources, cards, unread_count};

impl Sidebar {
    /// The inbox's cards, oldest first; `runs` say what woke an agent.
    pub fn inbox(&self, runs: &[Run]) -> Vec<InboxCard> {
        let data = &self.state.data;
        let mut task_names: std::collections::HashMap<String, String> = data
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.name.clone()))
            .collect();
        task_names.extend(data.idea_tasks.clone());
        let seen = &self.agent_markers.seen;
        let unread = |session: &str, completed: &str| seen.is_unread(session, Some(completed));
        cards(&InboxSources {
            agents: &data.agents,
            task_names: &task_names,
            runs,
            jobs: &self.pr_reviews,
            unread: &unread,
            now_ms: chrono::Utc::now().timestamp_millis(),
        })
    }

    /// Beside "Agentes": the unread cards, and whether one waits on the
    /// person (amber).
    pub(super) fn agents_waiting(&self) -> (usize, bool) {
        unread_count(&self.inbox(&[]))
    }

    /// A card opened: its result is read (a pull request review is read
    /// once its code tab shows it).
    pub fn mark_card_read(&mut self, card: &InboxCard, cx: &mut Context<Self>) {
        if let CardKind::TaskResult {
            session_id,
            completed_at,
            ..
        } = &card.kind
            && self.agent_markers.seen.mark_seen(session_id, completed_at)
        {
            cx.notify();
        }
    }

    /// "Marcar tudo como lido".
    pub fn mark_inbox_read(&mut self, cx: &mut Context<Self>) {
        for card in self.inbox(&[]).into_iter().filter(|card| card.unread) {
            match &card.kind {
                CardKind::PrReview { pr_id, .. } => {
                    crate::data::pr_review_jobs::PrReviewJobs::mark_seen(cx, pr_id)
                }
                _ => self.mark_card_read(&card, cx),
            }
        }
        cx.notify();
    }
}
