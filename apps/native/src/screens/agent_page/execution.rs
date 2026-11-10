//! What a resident agent runs on in the team room: its own provider and
//! model, else the composer's (`dcc_core::domain::agent_run`, the rule the
//! phone's room shares).

use dcc_core::domain::agent_run::{AgentRunError, agent_run_pick};

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
    let selected_model = composer.selected_model_id();
    match agent_run_pick(
        agent.provider_id.as_deref(),
        agent.model.as_deref(),
        &catalog,
        (selected.as_deref(), selected_model.as_deref()),
    ) {
        Ok(pick) => Ok((pick.provider_id, pick.model)),
        Err(AgentRunError::Unavailable) => Err(cx.t_with(
            "agents.call.providerUnavailable",
            &Args::default().with("agent", agent.name.clone()),
        )),
        Err(AgentRunError::Unhealthy(reason)) => Err(reason.into()),
    }
}
