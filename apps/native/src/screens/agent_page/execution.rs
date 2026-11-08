//! What a resident agent runs on in the team room: its own provider and
//! model, else the composer's.

use dcc_core::domain::provider::HealthStatus;

use crate::data::agents::AgentRecord;
use crate::i18n::{Args, Translate};

/// What an agent runs on in the team room:
/// its own provider and model, else the composer's (`handleCallAgent` with
/// a target, no kickoff).
pub(crate) fn agent_execution(
    agent: &AgentRecord,
    composer: &crate::screens::composer::Composer,
    cx: &gpui::App,
) -> Result<(String, Option<String>), gpui::SharedString> {
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
