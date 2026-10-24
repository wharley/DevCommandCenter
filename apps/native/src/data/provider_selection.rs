//! Which provider and model a task with no conversation starts on
//! (`provider-selection.logic.ts`): the project's last pick while that
//! provider is ready and still has the model, else the global pick, else a
//! ready (stable first) provider and its recommended model.

use dcc_core::domain::model_registry::resolve_alias;
use dcc_core::domain::provider::ProviderDescriptor;
use serde::{Deserialize, Serialize};

use super::model_picker::{is_enabled, is_ready};

/// A provider and model the person picked (`SessionComposerSelection`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub provider_id: String,
    pub model_id: String,
}

/// `resolveSelectedProviderId`
pub fn resolve_provider_id(
    providers: &[ProviderDescriptor],
    stored: Option<&str>,
) -> Option<String> {
    if let Some(stored) = stored
        && let Some(found) = providers.iter().find(|provider| provider.id.0 == stored)
    {
        return Some(found.id.0.clone());
    }
    // New work goes to a provider that can start; only when none is ready,
    // any switched-on one.
    let ready: Vec<&ProviderDescriptor> = providers
        .iter()
        .filter(|provider| is_ready(provider))
        .collect();
    let selectable: Vec<&ProviderDescriptor> = if ready.is_empty() {
        providers
            .iter()
            .filter(|provider| is_enabled(provider))
            .collect()
    } else {
        ready
    };
    selectable
        .iter()
        .find(|provider| provider.stable)
        .or_else(|| selectable.first())
        .map(|provider| provider.id.0.clone())
}

/// `resolveSelectedModelId`
pub fn resolve_model_id(provider: &ProviderDescriptor, stored: Option<&str>) -> Option<String> {
    if let Some(stored) = stored {
        let canonical = resolve_alias(&provider.id.0, stored);
        if let Some(found) = provider
            .models
            .iter()
            .find(|model| model.id == canonical || model.id == stored)
        {
            return Some(found.id.clone());
        }
    }
    provider
        .models
        .iter()
        .find(|model| model.recommended)
        .or_else(|| provider.models.first())
        .map(|model| model.id.clone())
}

/// `resolveProjectComposerSelection`
pub fn resolve_project_pick(
    providers: &[ProviderDescriptor],
    stored: Option<&Pick>,
) -> Option<Pick> {
    let stored = stored?;
    let provider = providers
        .iter()
        .find(|provider| provider.id.0 == stored.provider_id)
        .filter(|provider| is_ready(provider))?;
    Some(Pick {
        provider_id: provider.id.0.clone(),
        model_id: resolve_model_id(provider, Some(&stored.model_id))?,
    })
}

/// A new task's provider and model: the project's pick, else the global one.
pub fn new_task_pick(
    providers: &[ProviderDescriptor],
    project: Option<&Pick>,
    global: Option<&Pick>,
) -> Option<Pick> {
    if let Some(pick) = resolve_project_pick(providers, project) {
        return Some(pick);
    }
    let provider_id = resolve_provider_id(providers, global.map(|pick| pick.provider_id.as_str()))?;
    let provider = providers
        .iter()
        .find(|provider| provider.id.0 == provider_id)?;
    let stored_model = global
        .filter(|pick| pick.provider_id == provider_id)
        .map(|pick| pick.model_id.as_str());
    Some(Pick {
        model_id: resolve_model_id(provider, stored_model)?,
        provider_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::provider::HealthStatus;

    fn providers() -> Vec<ProviderDescriptor> {
        let capabilities = serde_json::json!({
            "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
            "resumable": true, "experimental": false, "canBeDelegationTarget": true,
            "canRequestDelegation": true, "supportsReadOnlyDelegation": true, "supportsEditDelegation": true,
            "approvalPolicies": ["auto"], "planModeSupport": "native"
        });
        let descriptor = |id: &str, stable: bool, models: serde_json::Value| {
            serde_json::from_value::<ProviderDescriptor>(serde_json::json!({
                "id": id, "label": id, "description": "", "models": models,
                "capabilities": capabilities, "health": "Healthy", "stable": stable
            }))
            .unwrap()
        };
        let model = |id: &str, recommended: bool| serde_json::json!({ "id": id, "label": id, "description": "", "recommended": recommended, "effortLevels": [] });
        vec![
            descriptor("cursor", false, serde_json::json!([model("auto", true)])),
            descriptor(
                "claude_code",
                true,
                serde_json::json!([
                    model("claude-haiku-5-5", false),
                    model("claude-sonnet-5-5", true)
                ]),
            ),
            descriptor(
                "codex",
                true,
                serde_json::json!([model("gpt-6.1-sol", true)]),
            ),
        ]
    }

    fn pick(provider: &str, model: &str) -> Pick {
        Pick {
            provider_id: provider.into(),
            model_id: model.into(),
        }
    }

    #[test]
    fn new_work_starts_on_a_ready_stable_provider_and_its_recommended_model() {
        let providers = providers();
        assert_eq!(
            new_task_pick(&providers, None, None),
            Some(pick("claude_code", "claude-sonnet-5-5"))
        );
        // A model of another provider does not travel.
        assert_eq!(
            new_task_pick(&providers, None, Some(&pick("codex", "claude-haiku-5-5"))),
            Some(pick("codex", "gpt-6.1-sol"))
        );
        assert_eq!(
            new_task_pick(
                &providers,
                None,
                Some(&pick("claude_code", "claude-haiku-5-5"))
            ),
            Some(pick("claude_code", "claude-haiku-5-5"))
        );
    }

    #[test]
    fn the_project_pick_wins_while_its_provider_is_ready() {
        let mut providers = providers();
        let project = pick("codex", "gpt-6.1-sol");
        let global = pick("claude_code", "claude-haiku-5-5");
        assert_eq!(
            new_task_pick(&providers, Some(&project), Some(&global)),
            Some(project.clone())
        );
        providers[2].health = HealthStatus::Degraded {
            reason: "no cli".into(),
        };
        assert_eq!(
            new_task_pick(&providers, Some(&project), Some(&global)),
            Some(global)
        );
        providers
            .iter_mut()
            .for_each(|provider| provider.enabled = false);
        assert_eq!(
            new_task_pick(&providers, None, None),
            None,
            "nothing switched on"
        );
    }
}
