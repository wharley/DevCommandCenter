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
                page.update(cx, |page, cx| page.show(id, cx));
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
                            shell.conversation.update(cx, |conversation, cx| {
                                conversation.open(&task_id, None, cx)
                            });
                            shell.show(Surface::Conversation, cx);
                        }
                        // `requestOpenPullRequest`
                        AgentPageEvent::OpenPullRequest(pr_id) => {
                            PrReviewJobs::request_open(cx, pr_id);
                            shell.open_pull_requests(window, cx);
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
    pub(super) fn open_pull_requests(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.pr_hub {
            Some((hub, ..)) => hub.update(cx, |hub, cx| hub.refresh(cx)),
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

impl Shell {
    /// An idea became a project: a task in its new folder opens, the composer
    /// focused (`createWorkspace` after `research_publish_idea`).
    pub(super) fn idea_published(
        &mut self,
        project_id: String,
        root_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(api) = (!cfg!(test))
            .then(crate::data::desktop_api::DesktopApi::load)
            .flatten()
        else {
            return;
        };
        let input = dcc_core::application::CreateWorkspaceForRepoInput {
            project_id: dcc_core::domain::project::ProjectId(project_id),
            workspace_root: root_path,
            base_branch: "main".into(),
            name: None,
            isolation_mode: None,
            context_folders: Vec::new(),
        };
        cx.spawn(async move |this, cx| {
            let created = cx
                .background_executor()
                .spawn(async move { api.create_workspace(&input) })
                .await;
            let _ = this.update(cx, |shell, cx| match created {
                Ok(task) => shell.open_created(task.workspace_id, cx),
                Err(error) => crate::ui::Toaster::error(error.to_string(), cx),
            });
        })
        .detach();
    }

    /// An idea was discarded: back to the Researcher's page.
    pub(super) fn idea_discarded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let researcher = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .find(|agent| {
                agent.preset.as_deref() == Some(crate::screens::agent_page::RESEARCHER_PRESET)
            })
            .map(|agent| agent.id.clone());
        match researcher {
            Some(id) => self.open_agent(id, window, cx),
            None => self.show(Surface::Conversation, cx),
        }
    }
}
