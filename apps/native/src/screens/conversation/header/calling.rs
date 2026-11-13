//! `handleCallAgent`: an agent called in the open task. It runs on its own
//! provider and model (the model it names when the provider has it, else
//! the recommended one) and its own account, or on the composer's; its
//! effort clamped to the model, else medium. The reviewer keeps one conversation per task: a
//! later round goes there, listing what the earlier one found. Any other
//! call starts a session named after the agent, bound to it (removed when
//! the binding fails), and sends its kickoff.

use dcc_core::domain::agent_call::call_block_key;
use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor};
use dcc_core::domain::review_findings::build_follow_up_kickoff;
use gpui::{Context, SharedString};

use crate::data::agents::{AgentRecord, latest_review};
use crate::data::composer_settings::clamp_effort;
use crate::data::conversation::Conversation;
use crate::data::desktop_api::{ApiError, TurnOptions};
use crate::i18n::{Args, Translate};
use crate::screens::conversation::ConversationView;
use crate::ui::Toaster;

/// `selectedProviderBlockReason`: why the composer's provider cannot take
/// new work here.
fn block_reason(
    provider: &ProviderDescriptor,
    data: &Conversation,
    cx: &gpui::App,
) -> Option<SharedString> {
    if let HealthStatus::Unhealthy { reason } = &provider.health {
        return Some(reason.clone().into());
    }
    let multi = data
        .new_thread
        .as_ref()
        .is_some_and(|start| !start.additional_workspace_ids.is_empty());
    (multi && !provider.capabilities.supports_multi_root).then(|| {
        cx.t_with(
            "workspaceScope.providerUnsupported",
            &Args::default().with("provider", provider.label.clone()),
        )
    })
}

/// The model it runs: the agent's when the provider has it, else the
/// recommended one, else the first.
fn agent_model(agent: &AgentRecord, provider: &ProviderDescriptor) -> Option<String> {
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
}

impl ConversationView {
    pub(crate) fn call_agent(&mut self, agent_id: String, cx: &mut Context<Self>) {
        if self.header.calling.is_some() {
            return;
        }
        let buttons = self.call_agents.read(cx);
        let Some(agent) = buttons
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
        else {
            return;
        };
        let (busy, has_changes) = (buttons.busy, buttons.has_changes);
        let Some(data) = self.data().cloned() else {
            Toaster::error(cx.t("agents.call.needsTask"), cx);
            return;
        };
        let Some(start) = data.new_thread.clone() else {
            Toaster::error(cx.t("agents.call.needsTask"), cx);
            return;
        };
        if let Some(key) = call_block_key(agent.preset.as_deref(), busy, has_changes) {
            Toaster::info(cx.t(key), cx);
            return;
        }
        let composer = self.composer.read(cx);
        let catalog = composer.catalog.clone().unwrap_or_default();
        let selected = composer.selected_provider_id();
        let wanted = agent.provider_id.clone().or_else(|| selected.clone());
        let provider = wanted
            .and_then(|id| catalog.iter().find(|provider| provider.id.0 == id).cloned())
            .filter(|provider| provider.enabled);
        let Some(provider) = provider else {
            let text = cx.t_with(
                "agents.call.providerUnavailable",
                &Args::default().with("agent", agent.name.clone()),
            );
            Toaster::error(text, cx);
            return;
        };
        let uses_selection =
            agent.provider_id.is_none() && selected.as_deref() == Some(provider.id.0.as_str());
        let blocked = uses_selection
            .then(|| block_reason(&provider, &data, cx))
            .flatten();
        if let Some(reason) = blocked {
            Toaster::error(reason, cx);
            return;
        }
        let model = if uses_selection {
            composer.selected_model_id()
        } else {
            agent_model(&agent, &provider)
        };
        // The agent's effort belongs to the agent's own model.
        let levels = provider
            .models
            .iter()
            .find(|candidate| Some(&candidate.id) == model.as_ref())
            .map(|candidate| candidate.effort_levels.clone())
            .unwrap_or_default();
        let effort = agent
            .effort
            .clone()
            .filter(|_| !uses_selection && !levels.is_empty())
            .map(|effort| clamp_effort(&effort, &levels))
            .unwrap_or_else(|| "medium".into());
        // The reviewer's conversation in this task, if it already has one.
        let review = (agent.preset.as_deref() == Some("reviewer"))
            .then(|| {
                agent.sessions.iter().find(|session| {
                    session.workspace_id == data.task_id
                        && data.sessions.iter().any(|row| row.id == session.session_id)
                })
            })
            .flatten()
            .map(|session| session.session_id.clone());
        let options = TurnOptions {
            provider_id: Some(provider.id.0.clone()),
            model: model.clone(),
            effort: Some(effort),
            approval_policy: Some("ask".into()),
            plan_mode: false,
            fast_mode: false,
            tool_instructions: None,
            // The agent's own account on its own provider (the Reviewer set to
            // the company's), not the composer's pick (`for_agent`).
            provider_runtime: crate::data::provider_runtime::for_agent(&agent, &provider.id.0)
                .or_else(|| Some(crate::data::provider_runtime::load().runtime_for(&provider))),
        };
        let intro = cx.t("agents.review.followUp").to_string();
        let previous_label = cx.t("agents.review.previousFindings").to_string();
        let severities: Vec<(dcc_core::domain::review_findings::ReviewSeverity, String)> = [
            dcc_core::domain::review_findings::ReviewSeverity::Critical,
            dcc_core::domain::review_findings::ReviewSeverity::Major,
            dcc_core::domain::review_findings::ReviewSeverity::Minor,
        ]
        .into_iter()
        .map(|severity| {
            let word = cx.t(&format!("agents.review.severity.{}", severity.id()));
            (severity, word.to_string())
        })
        .collect();
        let reviewer_kickoff = cx.t("agents.presets.reviewerKickoff").to_string();
        let failed = cx.t("agents.call.failed");
        let api = self.header_api(cx);
        let db = crate::data::loader::database_path();
        self.header.calling = Some(agent.id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let called = cx
                .background_executor()
                .spawn(async move {
                    let api = api.ok_or(ApiError::DesktopClosed)?;
                    let kickoff = agent.kickoff_prompt.trim().to_string();
                    if let Some(session) = review {
                        // No review there yet (its first run failed): start over.
                        let prompt = match latest_review(&db, &session) {
                            Some(previous) => build_follow_up_kickoff(
                                &previous,
                                &intro,
                                &previous_label,
                                |severity| {
                                    severities
                                        .iter()
                                        .find(|(candidate, _)| *candidate == severity)
                                        .map(|(_, word)| word.clone())
                                        .unwrap_or_default()
                                },
                            ),
                            None if kickoff.is_empty() => reviewer_kickoff,
                            None => kickoff,
                        };
                        api.send_turn(&session, &prompt, None, &options)?;
                        return Ok(session);
                    }
                    let session = api.start_thread(
                        &start,
                        &provider.id.0,
                        model.as_deref(),
                        Some(&agent.name),
                    )?;
                    if let Err(error) = api.bind_session_agent(&session, &agent.id) {
                        // Without the binding it is a plain session named after
                        // the agent, with no role: it goes rather than stays.
                        let _ = api.close_session(&session, true);
                        return Err(error);
                    }
                    if !kickoff.is_empty() {
                        api.send_turn(&session, &kickoff, None, &options)?;
                    }
                    Ok::<_, ApiError>(session)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.header.calling = None;
                match called {
                    Ok(session) => view.open_session(session, cx),
                    Err(ApiError::DesktopClosed) => Toaster::error(failed, cx),
                    Err(error) => Toaster::error(error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
