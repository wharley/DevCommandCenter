//! Which room the team room shows (`docs/PLANO_SALA_POR_PROJETO`): the
//! Geral room (none) or a project's, each with its own conversations. A
//! switch clears what was shown and reads the new room's; a read still on
//! its way from the room before is dropped by its generation.

use std::collections::HashMap;

use gpui::Context;

use super::room::TeamRoom;
use crate::data::check_ins::CheckIn;
use crate::data::room_inbox::InboxCard;
use crate::data::room_of::{card_project, room_for};
use crate::i18n::{Args, Translate};

impl TeamRoom {
    pub(crate) fn set_room(&mut self, room: Option<String>, cx: &mut Context<Self>) {
        if self.room == room {
            return;
        }
        let before = self.draft_key();
        self.room = room;
        self.room_generation += 1;
        self.feeds.clear();
        self.pending.clear();
        self.mention = None;
        // Its own draft comes back (`room_drafts`).
        self.swap_draft(before, cx);
        // The new room opens at its bottom (`room_list`).
        let placeholder = self.placeholder_text(cx);
        self.input
            .update(cx, |area, cx| area.set_placeholder(placeholder, cx));
        self.sync(cx);
        cx.notify();
    }

    /// The field's hint: the agent shown alone, the project's room, or the team.
    pub(crate) fn placeholder_text(&self, cx: &gpui::App) -> gpui::SharedString {
        if let Some(agent) = self.focused(cx) {
            return cx.t_with(
                "agents.room.placeholderAgent",
                &Args::default().with("agent", agent.name),
            );
        }
        match self.room_name(cx) {
            Some(project) => cx.t_with(
                "agents.room.placeholderProject",
                &Args::default().with("project", project),
            ),
            None => cx.t("agents.room.placeholder"),
        }
    }

    /// The name of the room's project, as the person sees it.
    pub(crate) fn room_name(&self, cx: &gpui::App) -> Option<String> {
        let room = self.room.as_deref()?;
        Some(
            self.sidebar
                .read(cx)
                .projects()
                .iter()
                .find(|project| project.project_id == room)
                .map(|project| project.display_name.clone())
                .unwrap_or_else(|| room.to_string()),
        )
    }

    /// The Geral room: DCC's own cards (results, reviews, the day's recap,
    /// the week, proposals, sources) live there for now.
    pub(crate) fn is_geral(&self) -> bool {
        self.room.is_none()
    }

    /// The room `card` shows in: its project's, or Geral.
    pub(crate) fn card_room(&self, card: &InboxCard, cx: &gpui::App) -> Option<String> {
        let labels: HashMap<String, String> = crate::data::pr_review_jobs::PrReviewJobs::all(cx)
            .into_iter()
            .map(|job| (job.pr_id, job.label))
            .collect();
        let sidebar = self.sidebar.read(cx);
        room_for(
            card_project(card, sidebar.tasks(), sidebar.projects(), &labels),
            &self.rooms,
        )
    }

    /// The room a check-in shows in.
    pub(crate) fn check_in_room(&self, check: &CheckIn) -> Option<String> {
        room_for(check.project_id.clone(), &self.rooms)
    }

    /// The unread cards and check-ins of each room (none: Geral), for the
    /// dots in the column.
    pub(crate) fn unread_by_room(&self, cx: &gpui::App) -> HashMap<Option<String>, usize> {
        let mut unread: HashMap<Option<String>, usize> = HashMap::new();
        for card in self.cards(cx).iter().filter(|card| card.unread) {
            let room = self.card_room(card, cx);
            if !self.asked_in(card, &room, cx) {
                *unread.entry(room).or_default() += 1;
            }
        }
        for check in self.check_ins(cx).iter().filter(|check| check.unread) {
            *unread.entry(self.check_in_room(check)).or_default() += 1;
        }
        unread
    }

    /// A result of a task an agent opened, in the room it was asked in:
    /// that conversation already carries its report, so no card there.
    pub(crate) fn asked_in(&self, card: &InboxCard, room: &Option<String>, cx: &gpui::App) -> bool {
        let crate::data::room_inbox::CardKind::TaskResult { workspace_id, .. } = &card.kind else {
            return false;
        };
        let sidebar = self.sidebar.read(cx);
        let Some(opened) = sidebar.opened_tasks().get(workspace_id) else {
            return false;
        };
        sidebar
            .agents()
            .iter()
            .flat_map(|agent| agent.sessions.iter())
            .find(|session| session.session_id == opened.session_id)
            .is_some_and(|session| session.room == *room)
    }
}
