//! Which room the team room shows (`docs/PLANO_SALA_POR_PROJETO`): the
//! Geral room (none) or a project's, each with its own conversations. A
//! switch clears what was shown and reads the new room's; a read still on
//! its way from the room before is dropped by its generation.

use gpui::Context;

use super::room::TeamRoom;
use crate::i18n::{Args, Translate};

impl TeamRoom {
    pub(crate) fn set_room(&mut self, room: Option<String>, cx: &mut Context<Self>) {
        if self.room == room {
            return;
        }
        self.room = room;
        self.room_generation += 1;
        self.feeds.clear();
        self.pending.clear();
        self.mention = None;
        // The new room opens at its bottom.
        self.shown = usize::MAX;
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
}
