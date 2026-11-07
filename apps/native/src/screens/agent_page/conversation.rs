//! "Conversa" on an agent's page: the person talks to the agent in the team
//! room ("Falar na Sala", with its name addressed); its own conversation, in
//! its home (`data::agent_conversation`), opens whole from here once it has
//! one. The room makes the home, its task and the conversation the first
//! time.

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::{AgentRecord, AgentSession};
use crate::i18n::Translate;
use crate::screens::sidebar::AgentPageData;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon};

/// The agent's latest conversation in its home, if it has one.
pub(crate) fn home_session(agent: &AgentRecord) -> Option<&AgentSession> {
    let home = home_project_id(&agent.id);
    agent
        .sessions
        .iter()
        .filter(|session| session.project_id == home)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
}

impl AgentPage {
    /// The agent's own conversation, whole, with its tools.
    pub(crate) fn open_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        if let Some(session) = home_session(&data.agent) {
            cx.emit(AgentPageEvent::OpenSession {
                task_id: session.workspace_id.clone(),
                session_id: session.session_id.clone(),
            });
        }
    }

    pub(super) fn conversation_card(
        &self,
        data: &AgentPageData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let existing = home_session(&data.agent).is_some();
        let name = data.agent.name.clone();
        Self::card(cx)
            .debug_selector(|| "agent-conversation".into())
            .flex()
            .items_center()
            .gap(px(16.0))
            .p(px(20.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                Icon::new("message-circle")
                                    .size(16.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(cx.t("agents.conversation.title")),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.conversation.hint")),
                    ),
            )
            .when(existing, |card| {
                card.child(
                    Button::new("agent-conversation-open")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Default)
                        .text_size(14.0)
                        .on_click(cx.listener(|page, _, _, cx| page.open_conversation(cx)))
                        .child(cx.t("agents.conversation.continue")),
                )
            })
            .child(
                Button::new("agent-conversation-room")
                    .size(ButtonSize::Default)
                    .text_size(14.0)
                    .on_click(cx.listener(move |page, _, _, cx| {
                        page.open_room(Some(format!("@{name} ")), cx)
                    }))
                    .child(cx.t("agents.conversation.inRoom")),
            )
            .into_any_element()
    }
}
