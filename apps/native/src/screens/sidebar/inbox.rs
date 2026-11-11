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
        let task_names: std::collections::HashMap<String, String> = data
            .tasks
            .iter()
            .map(|task| (task.id.clone(), task.name.clone()))
            .collect();
        let seen = &self.agent_markers.seen;
        let unread = |session: &str, completed: &str| seen.is_unread(session, Some(completed));
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut all = cards(&InboxSources {
            agents: &data.agents,
            task_names: &task_names,
            runs,
            jobs: &self.pr_reviews,
            unread: &unread,
            now_ms,
        });
        // The tasks agents opened, once done (`room_opened`).
        all.extend(crate::data::room_opened::opened_cards(
            &data.opened_tasks,
            &data.tasks,
            &data.activities,
            &unread,
            now_ms,
        ));
        all.sort_by(|left, right| left.at.cmp(&right.at));
        all
    }

    /// The tasks agents opened, with the conversation each came from.
    pub fn opened_tasks(
        &self,
    ) -> &std::collections::HashMap<String, crate::data::task_agents::OpenedTask> {
        &self.state.data.opened_tasks
    }

    /// Beside "Agentes": the unread cards (the day's recap, the check-in's
    /// and the replies to send among them), and whether one waits on the
    /// person (amber).
    pub(crate) fn agents_waiting(&self, cx: &gpui::App) -> (usize, bool) {
        let (count, waiting) = unread_count(&self.inbox(&[]));
        let checks = cx
            .try_global::<crate::data::check_ins::CheckIns>()
            .map_or(0, |checks| checks.unread());
        // A reply an agent proposed waits on the person to send it.
        let replies = self.replies_waiting;
        (
            count
                + usize::from(self.recap_unread())
                + usize::from(self.bulletin_unread())
                + checks
                + replies,
            waiting || replies > 0,
        )
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
        self.agent_markers.recap.mark_seen();
        self.agent_markers.bulletin.mark_seen(chrono::Local::now());
        if cx.has_global::<crate::data::check_ins::CheckIns>() {
            gpui::BorrowAppContext::update_global::<crate::data::check_ins::CheckIns, _>(
                cx,
                |checks, _| checks.mark_all_read(),
            );
        }
        cx.notify();
    }
}
