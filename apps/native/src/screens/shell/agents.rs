//! The pages in the center (`globalSurface`): a DCC agent's, opened from the
//! sidebar's mascots (the agent stays marked there while it shows, and a
//! conversation picked on it opens its task), and "Pull Requests".

use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Subscription, Window};

use super::{Shell, Surface};
use crate::data::pr_review_jobs::PrReviewJobs;
use crate::screens::agent_page::{AgentPage, AgentPageEvent};
use crate::screens::pr_hub::{PrHub, PrHubEvent};

pub(super) type OpenAgentPage = (Entity<AgentPage>, Subscription);
pub(super) type OpenPrHub = (Entity<PrHub>, Subscription, Vec<Subscription>);

impl Shell {
    /// `handleOpenAgent`
    pub(super) fn open_agent(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        match &self.agent_page {
            Some((page, _)) => {
                let id = id.clone();
                page.update(cx, |page, cx| page.show_conversation(id, cx));
            }
            None => {
                let (sidebar, composer) = (
                    self.sidebar.clone(),
                    self.conversation.read(cx).composer.clone(),
                );
                let id = id.clone();
                let page = cx.new(|cx| AgentPage::new(id, sidebar, composer, cx));
                let events =
                    cx.subscribe_in(&page, window, |shell, _, event, window, cx| match event {
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
                        AgentPageEvent::OpenTask(task_id) => {
                            let task_id = task_id.clone();
                            shell.conversation.update(cx, |conversation, cx| {
                                conversation.open(&task_id, None, cx)
                            });
                            shell.show(Surface::Conversation, cx);
                        }
                        AgentPageEvent::OpenTaskChanges(task_id) => {
                            shell.open_task_changes(task_id, cx)
                        }
                        AgentPageEvent::DeliverTask { task_id, step } => {
                            shell.deliver_task(task_id, *step, cx)
                        }
                        // `requestOpenPullRequest`
                        AgentPageEvent::OpenPullRequest(pr_id) => {
                            PrReviewJobs::request_open(cx, pr_id);
                            shell.open_pull_requests(window, cx);
                        }
                    });
                page.update(cx, |page, cx| {
                    page.show_conversation(page.agent_id.clone(), cx)
                });
                self.agent_page = Some((page, events));
            }
        }
        self.show(Surface::Agent, cx);
    }

    /// "Agentes": the team's screen as it was left, the first time on the
    /// team room.
    pub(super) fn open_agents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agent_page.is_some() {
            self.show(Surface::Agent, cx);
            return;
        }
        let Some(id) = self.sidebar.read(cx).lead_agent() else {
            return;
        };
        self.open_agent(id, window, cx);
        if let Some((page, _)) = &self.agent_page {
            page.update(cx, |page, cx| page.open_room(None, cx));
        }
    }

    pub(super) fn agent_page_element(&self) -> AnyElement {
        self.agent_page
            .as_ref()
            .map(|(page, _)| page.clone().into_any_element())
            .unwrap_or_else(|| gpui::div().into_any_element())
    }

    /// "Pull Requests" from the sidebar: read again each time it opens.
    pub(super) fn open_pull_requests(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.pr_hub {
            Some((hub, ..)) => hub.update(cx, |hub, cx| hub.reopen(cx)),
            None => {
                let hub = cx.new(|cx| PrHub::new(window, cx));
                let composer = self.conversation.read(cx).composer.clone();
                let sources = vec![
                    cx.observe(&self.sidebar, |shell, _, cx| shell.feed_pr_hub(cx)),
                    cx.observe(&composer, |shell, _, cx| shell.feed_pr_hub(cx)),
                ];
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
                self.pr_hub = Some((hub, events, sources));
            }
        }
        // The reviewer runs on the composer's providers.
        let composer = self.conversation.read(cx).composer.clone();
        composer.update(cx, |composer, cx| composer.refresh_catalog(cx));
        self.feed_pr_hub(cx);
        if let Some((hub, ..)) = &self.pr_hub {
            hub.update(cx, |hub, cx| hub.take_requested(cx));
        }
        self.show(Surface::PullRequests, cx);
    }

    /// The reviewer, the providers and the composer's pick, for the hub.
    fn feed_pr_hub(&mut self, cx: &mut Context<Self>) {
        let Some((hub, ..)) = &self.pr_hub else {
            return;
        };
        let reviewer = self.sidebar.read(cx).reviewer();
        let composer = self.conversation.read(cx).composer.read(cx);
        let providers = composer.catalog.clone().unwrap_or_default();
        let pick = (
            composer.session.provider_id.clone(),
            composer.session.model.clone(),
        );
        hub.update(cx, |hub, cx| hub.set_sources(reviewer, providers, pick, cx));
    }

    pub(super) fn pr_hub_element(&self) -> AnyElement {
        self.pr_hub
            .as_ref()
            .map(|(hub, ..)| hub.clone().into_any_element())
            .unwrap_or_else(|| gpui::div().into_any_element())
    }
}
