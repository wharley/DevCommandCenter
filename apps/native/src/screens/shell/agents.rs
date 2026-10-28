//! The pages in the center (`globalSurface`): a DCC agent's, opened from the
//! sidebar's mascots (the agent stays marked there while it shows, and a
//! conversation picked on it opens its task), and "Pull Requests".

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription};

use super::{Shell, Surface};
use crate::screens::agent_page::{AgentPage, AgentPageEvent};
use crate::screens::pr_hub::{PrHub, PrHubEvent};

pub(super) type OpenAgentPage = (Entity<AgentPage>, Subscription);
pub(super) type OpenPrHub = (Entity<PrHub>, Subscription);

impl Shell {
    /// `handleOpenAgent`
    pub(super) fn open_agent(&mut self, id: String, cx: &mut Context<Self>) {
        match &self.agent_page {
            Some((page, _)) => {
                let id = id.clone();
                page.update(cx, |page, cx| page.show(id, cx));
            }
            None => {
                let (sidebar, composer) = (
                    self.sidebar.clone(),
                    self.conversation.read(cx).composer.clone(),
                );
                let id = id.clone();
                let page = cx.new(|cx| AgentPage::new(id, sidebar, composer, cx));
                let events = cx.subscribe(&page, |shell, _, event, cx| match event {
                    AgentPageEvent::OpenSession {
                        task_id,
                        session_id,
                    } => {
                        let (task_id, session_id) = (task_id.clone(), session_id.clone());
                        shell.conversation.update(cx, |conversation, cx| {
                            conversation.open(&task_id, Some(session_id), cx)
                        });
                        shell.show(Surface::Conversation, cx);
                    }
                    AgentPageEvent::IdeaStarted {
                        task_id,
                        session_id,
                    } => {
                        let (task_id, session_id) = (task_id.clone(), session_id.clone());
                        shell.conversation.update(cx, |conversation, cx| {
                            conversation.open(&task_id, Some(session_id), cx);
                            conversation.focus_composer_soon(cx);
                        });
                        shell.show(Surface::Conversation, cx);
                    }
                    AgentPageEvent::OpenTask(task_id) => {
                        let task_id = task_id.clone();
                        shell
                            .conversation
                            .update(cx, |conversation, cx| conversation.open(&task_id, None, cx));
                        shell.show(Surface::Conversation, cx);
                    }
                });
                self.agent_page = Some((page, events));
            }
        }
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_active_agent(Some(id), cx));
        self.show(Surface::Agent, cx);
    }

    pub(super) fn agent_page_element(&self) -> AnyElement {
        self.agent_page
            .as_ref()
            .map(|(page, _)| page.clone().into_any_element())
            .unwrap_or_else(|| gpui::div().into_any_element())
    }

    /// "Pull Requests" from the sidebar: read again each time it opens.
    pub(super) fn open_pull_requests(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        match &self.pr_hub {
            Some((hub, _)) => hub.update(cx, |hub, cx| hub.refresh(cx)),
            None => {
                let hub = cx.new(|cx| PrHub::new(window, cx));
                let events = cx.subscribe(&hub, |shell, _, event, cx| match event {
                    PrHubEvent::OpenTask(task_id) => {
                        let task_id = task_id.clone();
                        shell
                            .conversation
                            .update(cx, |conversation, cx| conversation.open(&task_id, None, cx));
                        shell.show(Surface::Conversation, cx);
                    }
                    // The new task opens once the list has it, its composer in focus.
                    PrHubEvent::Created(task_id) => shell.open_created(task_id.clone(), cx),
                });
                self.pr_hub = Some((hub, events));
            }
        }
        self.show(Surface::PullRequests, cx);
    }

    pub(super) fn pr_hub_element(&self) -> AnyElement {
        self.pr_hub
            .as_ref()
            .map(|(hub, _)| hub.clone().into_any_element())
            .unwrap_or_else(|| gpui::div().into_any_element())
    }
}
