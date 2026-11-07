//! "Conversa" on an agent's page: one ongoing conversation with the agent,
//! in its home (`data::agent_conversation`), where it answers about the
//! person's tasks with DCC's tools. The first "Conversar" makes the home
//! and its task; later ones open the latest conversation there.

use dcc_core::domain::agent_conversation::home_project_id;
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use super::{AgentPage, AgentPageEvent};
use crate::data::agent_conversation::ConversationStart;
use crate::data::agents::{AgentRecord, AgentSession};
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::AgentPageData;
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, ButtonVariant, Icon, Toaster, spinner};

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
    pub(crate) fn open_conversation(&mut self, cx: &mut Context<Self>) {
        if self.starting_conversation {
            return;
        }
        let Some(data) = self.sidebar.read(cx).agent_page(&self.agent_id) else {
            return;
        };
        if let Some(session) = home_session(&data.agent) {
            cx.emit(AgentPageEvent::OpenSession {
                task_id: session.workspace_id.clone(),
                session_id: session.session_id.clone(),
            });
            return;
        }
        let agent = data.agent;
        let (provider, model) = match self.agent_execution(&agent, cx) {
            Ok(execution) => execution,
            Err(reason) => {
                Toaster::error(reason, cx);
                return;
            }
        };
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        let title = cx
            .t_with(
                "agents.conversation.taskName",
                &Args::default().with("agent", agent.name.clone()),
            )
            .to_string();
        let home_task = data.home_task;
        self.starting_conversation = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    api.start_agent_conversation(&ConversationStart {
                        agent_id: &agent.id,
                        agent_name: &agent.name,
                        home_task: home_task.as_deref(),
                        title: &title,
                        provider: &provider,
                        model: model.as_deref(),
                    })
                })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.starting_conversation = false;
                match started {
                    // A new conversation opens with the composer in focus.
                    Ok((task_id, session_id)) => cx.emit(AgentPageEvent::IdeaStarted {
                        task_id,
                        session_id,
                    }),
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn conversation_card(
        &self,
        data: &AgentPageData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let starting = self.starting_conversation;
        let existing = home_session(&data.agent).is_some();
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
            .child(
                Button::new("agent-conversation-open")
                    .variant(if existing {
                        ButtonVariant::Outline
                    } else {
                        ButtonVariant::Default
                    })
                    .size(ButtonSize::Default)
                    .text_size(14.0)
                    .disabled(starting)
                    .on_click(cx.listener(|page, _, _, cx| page.open_conversation(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(starting, |label| {
                                label.child(spinner("agent-conversation-starting", 16.0))
                            })
                            .child(cx.t(if existing {
                                "agents.conversation.continue"
                            } else {
                                "agents.conversation.start"
                            })),
                    ),
            )
            .into_any_element()
    }
}
