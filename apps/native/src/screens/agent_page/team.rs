//! The team's screen ("Agentes" in the sidebar): the agents down the left,
//! each with its mascot as it is and its mood, and "Novo agente"; the page
//! of the one picked in the center, or the team room (`room`) on top.

use gpui::{
    AnyElement, AppContext, Context, IntoElement, Render, SharedString, Window, div, prelude::*, px,
};

use super::{AgentPage, AgentPageEvent, room};
use crate::agent::{Mood, MoodAvatarView};
use crate::data::agents::AgentState;
use crate::i18n::Translate;
use crate::theme::ActiveTheme;
use crate::ui::Icon;

/// The column's width.
const TEAM_WIDTH: f32 = 224.0;

impl AgentPage {
    /// The team room, with `draft` in its field when given.
    pub fn open_room(&mut self, draft: Option<String>, cx: &mut Context<Self>) {
        self.room_open = true;
        // The Geral room; `open_room_of` picks a project's.
        self.room_project = None;
        self.editor = None;
        self.room_draft = draft;
        // The limit may have changed in Settings meanwhile.
        if let Some((room, _)) = &self.room {
            room.update(cx, |room, cx| {
                room.reload_cleanup();
                room.read_limits(cx);
            });
        }
        cx.notify();
    }

    /// The room, on the whole team or on one agent (`focus`).
    pub(super) fn room_element(
        &mut self,
        focus: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let room = match &self.room {
            Some((room, _)) => room.clone(),
            None => {
                let (sidebar, composer) = (self.sidebar.clone(), self.composer.clone());
                let room = cx.new(|cx| room::TeamRoom::new(sidebar, composer, window, cx));
                let events =
                    cx.subscribe_in(&room, window, |page, _, event, window, cx| match event {
                        room::RoomEvent::OpenSession {
                            task_id,
                            session_id,
                        } => cx.emit(AgentPageEvent::OpenSession {
                            task_id: task_id.clone(),
                            session_id: session_id.clone(),
                        }),
                        room::RoomEvent::OpenPullRequest(pr_id) => {
                            cx.emit(AgentPageEvent::OpenPullRequest(pr_id.clone()))
                        }
                        room::RoomEvent::OpenTask(task_id) => {
                            cx.emit(AgentPageEvent::OpenTask(task_id.clone()))
                        }
                        room::RoomEvent::OpenAgent(agent_id) => page.show(agent_id.clone(), cx),
                        room::RoomEvent::AgentCreated {
                            agent_id,
                            first_request,
                        } => {
                            if !cfg!(test) {
                                page.sidebar
                                    .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                            }
                            page.show_conversation(agent_id.clone(), cx);
                            page.room_draft = Some(first_request.clone());
                        }
                        room::RoomEvent::EditProposal(proposal) => {
                            page.edit_proposal(proposal.clone(), window, cx)
                        }
                    });
                self.room = Some((room.clone(), events));
                room
            }
        };
        // One agent's "Conversa" shows the room picked for it (slice 5).
        let project = if focus.is_some() {
            self.agent_room(cx)
        } else {
            self.room_project.clone()
        };
        let rooms: Vec<String> = {
            let sidebar = self.sidebar.read(cx);
            let projects = super::editor_projects::project_options(sidebar.projects());
            let talked = crate::data::agent_rooms::talked(sidebar.agents());
            crate::data::agent_rooms::rooms(&self.rooms, talked, &projects)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        room.update(cx, |room, cx| {
            room.rooms = rooms;
            room.set_room(project, cx);
            room.set_focus(focus, cx);
        });
        if let Some(draft) = self.room_draft.take() {
            let input = room.read(cx).input.clone();
            input.update(cx, |area, cx| area.set_text(&draft, cx));
            let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
            window.focus(&handle, cx);
        }
        room.into_any_element()
    }

    fn team_row(
        &self,
        index: usize,
        id: String,
        name: String,
        avatar: &dcc_core::domain::agent::AgentAvatar,
        mood: Mood,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let current = !self.room_open && self.agent_id == id && self.editor_is_for_agent(cx);
        let mood_text = cx.t(mood.i18n_key());
        let urgent = mood == Mood::NeedsYou;
        div()
            .id(SharedString::from(format!("team-agent-{index}")))
            .debug_selector(move || format!("team-agent-{index}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(cx.theme().radius.control)
            .cursor_pointer()
            .when(current, |row| row.bg(colors.accent))
            .when(!current, |row| {
                row.hover(move |style| style.bg(colors.accent.opacity(0.5)))
            })
            .on_click(cx.listener(move |page, _, _, cx| page.show_conversation(id.clone(), cx)))
            .child(MoodAvatarView::new(
                SharedString::from(format!("team-face-{index}")),
                avatar,
                mood,
                30.0,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(if urgent {
                                crate::theme::tailwind::AMBER_600.hsla()
                            } else {
                                colors.muted_foreground
                            })
                            .child(mood_text),
                    ),
            )
            .into_any_element()
    }

    /// The agent picked is shown (not "Novo agente" in the editor).
    pub(super) fn editor_is_for_agent(&self, cx: &gpui::App) -> bool {
        self.editor
            .as_ref()
            .is_none_or(|(editor, _)| !editor.read(cx).is_new())
    }

    fn team_column(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let sidebar = self.sidebar.read(cx);
        let agents: Vec<_> = sidebar
            .agents()
            .iter()
            .map(|agent| {
                let mood = sidebar
                    .mood(&agent.id, cx)
                    .unwrap_or_else(|| Mood::of(AgentState::Idle, false, false));
                (
                    agent.id.clone(),
                    agent.name.clone(),
                    agent.avatar.clone(),
                    mood,
                )
            })
            .collect();
        let rows: Vec<AnyElement> = agents
            .into_iter()
            .enumerate()
            .map(|(index, (id, name, avatar, mood))| {
                self.team_row(index, id, name, &avatar, mood, cx)
            })
            .collect();
        let creating = !self.editor_is_for_agent(cx);
        let new_agent = div()
            .id("team-new-agent")
            .debug_selector(|| "team-new-agent".into())
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .h(px(34.0))
            .rounded(cx.theme().radius.control)
            .cursor_pointer()
            .text_size(px(13.0))
            .text_color(colors.muted_foreground)
            .when(creating, |row| {
                row.bg(colors.accent).text_color(colors.foreground)
            })
            .when(!creating, |row| {
                row.hover(move |style| {
                    style
                        .bg(colors.accent.opacity(0.5))
                        .text_color(colors.foreground)
                })
            })
            .on_click(cx.listener(|page, _, window, cx| page.open_new_agent(window, cx)))
            .child(
                div()
                    .flex()
                    .size(px(30.0))
                    .items_center()
                    .justify_center()
                    .child(Icon::new("plus").size(16.0)),
            )
            .child(cx.t("agents.team.new"));
        div()
            .id("team-column")
            .debug_selector(|| "team-column".into())
            .flex()
            .flex_col()
            .flex_none()
            .w(px(TEAM_WIDTH))
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .gap(px(2.0))
            .px(px(8.0))
            .py(px(16.0))
            .border_r_1()
            .border_color(colors.border.opacity(0.7))
            .children(self.rooms_section(cx))
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(16.0))
                    .child(AgentPage::section_title(cx.t("agents.team.title"), cx)),
            )
            .children(rows)
            .child(new_agent)
            .into_any_element()
    }
}

impl Render for AgentPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.page_body(window, cx);
        div()
            .flex()
            .size_full()
            .min_h_0()
            .bg(cx.theme().colors.background)
            .child(self.team_column(cx))
            .child(div().flex_1().min_w_0().h_full().child(body))
    }
}
