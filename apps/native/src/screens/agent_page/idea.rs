//! The Researcher's "Nova ideia" (`NewIdeaCard`, `handleStartIdea`): DCC
//! makes the idea's folder in `~/dcc-ideias`, a task straight on it (it is
//! isolated already), and an empty conversation with the Researcher where
//! the person describes the idea; the task has no name yet, so the first
//! message names it.

use dcc_core::application::linked_task::infer_project_id_from_workspace_root;
use dcc_core::application::{CreateWorkspaceForRepoInput, WorkspaceIsolationMode};
use dcc_core::domain::project::ProjectId;
use dcc_core::domain::provider::HealthStatus;
use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};
use serde::Deserialize;
use serde_json::json;

use super::{AgentPage, AgentPageEvent};
use crate::data::agents::AgentRecord;
use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::{Args, Translate};
use crate::theme::ActiveTheme;
use crate::ui::{Button, ButtonSize, Icon, Toaster, spinner};

/// `Idea`
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Idea {
    pub root_path: String,
}

impl DesktopApi {
    /// `research_create_idea`
    pub fn create_idea(&self) -> Result<Idea, ApiError> {
        let value = self.post("/api/v1/research/ideas", &json!({}))?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }
}

/// The idea's project id: its folder's, with the time in base 36 (the
/// folder name comes back once an idea leaves; the id never does).
pub fn idea_project_id(root: &str, now_ms: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut value = now_ms;
    let mut base36 = Vec::new();
    loop {
        base36.push(DIGITS[(value % 36) as usize]);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    base36.reverse();
    format!(
        "{}-{}",
        infer_project_id_from_workspace_root(root),
        String::from_utf8(base36).unwrap_or_default()
    )
}

impl AgentPage {
    /// What the Researcher runs on: its own provider and model, else the
    /// composer's (`handleCallAgent` with a target, no kickoff).
    fn idea_execution(
        &self,
        agent: &AgentRecord,
        cx: &gpui::App,
    ) -> Result<(String, Option<String>), gpui::SharedString> {
        let composer = self.composer.read(cx);
        let catalog = composer.catalog.clone().unwrap_or_default();
        let selected = composer.selected_provider_id();
        let wanted = agent.provider_id.clone().or_else(|| selected.clone());
        let provider = wanted
            .and_then(|id| catalog.iter().find(|provider| provider.id.0 == id).cloned())
            .filter(|provider| provider.enabled)
            .ok_or_else(|| {
                cx.t_with(
                    "agents.call.providerUnavailable",
                    &Args::default().with("agent", agent.name.clone()),
                )
            })?;
        let uses_selection =
            agent.provider_id.is_none() && selected.as_deref() == Some(provider.id.0.as_str());
        if uses_selection && let HealthStatus::Unhealthy { reason } = &provider.health {
            return Err(reason.clone().into());
        }
        let model = if uses_selection {
            composer.selected_model_id()
        } else {
            agent
                .model
                .clone()
                .filter(|model| {
                    provider
                        .models
                        .iter()
                        .any(|candidate| &candidate.id == model)
                })
                .or_else(|| {
                    provider
                        .models
                        .iter()
                        .find(|model| model.recommended)
                        .map(|model| model.id.clone())
                })
                .or_else(|| provider.models.first().map(|model| model.id.clone()))
        };
        Ok((provider.id.0.clone(), model))
    }

    pub(crate) fn start_idea(&mut self, cx: &mut Context<Self>) {
        if self.starting_idea {
            return;
        }
        let Some(agent) = self
            .sidebar
            .read(cx)
            .agent_page(&self.agent_id)
            .map(|data| data.agent)
        else {
            Toaster::error(cx.t("agents.research.missing"), cx);
            return;
        };
        let (provider, model) = match self.idea_execution(&agent, cx) {
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
        self.starting_idea = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let idea = api.create_idea()?;
                    let project =
                        idea_project_id(&idea.root_path, crate::data::time::now_ms() as u64);
                    let created = api.create_workspace(&CreateWorkspaceForRepoInput {
                        project_id: ProjectId(project.clone()),
                        workspace_root: idea.root_path.clone(),
                        base_branch: "main".into(),
                        name: None,
                        isolation_mode: Some(WorkspaceIsolationMode::LocalDirect),
                        context_folders: Vec::new(),
                    })?;
                    let start = ThreadStart {
                        workspace_id: created.workspace_id.clone(),
                        project_id: project,
                        additional_workspace_ids: Vec::new(),
                        name: None,
                        auto_named: true,
                    };
                    let session =
                        api.start_thread(&start, &provider, model.as_deref(), Some(&agent.name))?;
                    if let Err(error) = api.bind_session_agent(&session, &agent.id) {
                        // Without the binding it is a plain session named after the
                        // agent, with no role: it goes rather than stays.
                        let _ = api.close_session(&session, true);
                        return Err(error);
                    }
                    Ok::<_, ApiError>((created.workspace_id, session))
                })
                .await;
            let _ = this.update(cx, |page, cx| {
                page.starting_idea = false;
                match started {
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

    /// `NewIdeaCard`
    pub(super) fn new_idea_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors.clone();
        let starting = self.starting_idea;
        Self::card(cx)
            .debug_selector(|| "agent-new-idea".into())
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
                                Icon::new("lightbulb")
                                    .size(16.0)
                                    .color(colors.muted_foreground),
                            )
                            .child(cx.t("agents.research.newIdea")),
                    )
                    .child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(19.5))
                            .text_color(colors.muted_foreground)
                            .child(cx.t("agents.research.newIdeaHint")),
                    ),
            )
            .child(
                Button::new("agent-new-idea-start")
                    .size(ButtonSize::Default)
                    .text_size(14.0)
                    .disabled(starting)
                    .on_click(cx.listener(|page, _, _, cx| page.start_idea(cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when(starting, |label| {
                                label.child(spinner("agent-new-idea-starting", 16.0))
                            })
                            .child(cx.t("agents.research.start")),
                    ),
            )
            .into_any_element()
    }
}
