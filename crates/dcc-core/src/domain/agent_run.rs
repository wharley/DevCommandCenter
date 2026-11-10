//! What a resident agent runs on when the person talks to it in a room: its
//! own provider and model, else the provider and model picked where the
//! message is written (the desktop's composer; on the phone, what the agent
//! ran on last). One rule for the desktop's room and the phone's.

use crate::domain::provider::{HealthStatus, ProviderDescriptor};
use crate::ports::provider::ProviderRuntimeConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRunPick {
    pub provider_id: String,
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentRunError {
    /// Neither its provider nor the picked one is there and enabled.
    Unavailable,
    /// The picked provider is unhealthy, with its reason.
    Unhealthy(String),
}

/// The provider and model of a room's turn for an agent with
/// `agent_provider`/`agent_model`, among `providers`, with `picked` the
/// `(provider, model)` chosen where the message was written.
pub fn agent_run_pick(
    agent_provider: Option<&str>,
    agent_model: Option<&str>,
    providers: &[ProviderDescriptor],
    picked: (Option<&str>, Option<&str>),
) -> Result<AgentRunPick, AgentRunError> {
    let wanted = agent_provider.or(picked.0);
    let provider = wanted
        .and_then(|id| providers.iter().find(|provider| provider.id.0 == id))
        .filter(|provider| provider.enabled)
        .ok_or(AgentRunError::Unavailable)?;
    let uses_pick = agent_provider.is_none() && picked.0 == Some(provider.id.0.as_str());
    if uses_pick {
        if let HealthStatus::Unhealthy { reason } = &provider.health {
            return Err(AgentRunError::Unhealthy(reason.clone()));
        }
    }
    let model = if uses_pick {
        picked.1.map(str::to_string)
    } else {
        agent_model
            .filter(|model| provider.models.iter().any(|known| known.id == *model))
            .map(str::to_string)
            .or_else(|| {
                provider
                    .models
                    .iter()
                    .find(|model| model.recommended)
                    .map(|model| model.id.clone())
            })
            .or_else(|| provider.models.first().map(|model| model.id.clone()))
    };
    Ok(AgentRunPick {
        provider_id: provider.id.0.clone(),
        model,
    })
}

/// The effort a room's turn asks: the agent's, only on its own provider.
pub fn agent_run_effort(agent_provider: Option<&str>, effort: Option<&str>) -> Option<String> {
    agent_provider.and(effort).map(str::to_string)
}

/// The runtime a room's turn runs with: `base` (the conversation's own, or
/// for a new one the agent's latest on that provider) with, on the agent's
/// own provider that has accounts, the agent's account (`own_account`;
/// `Some(None)` is the system login). `None` keeps the conversation's.
pub fn agent_run_runtime(
    base: Option<ProviderRuntimeConfig>,
    own_account: Option<Option<String>>,
) -> Option<ProviderRuntimeConfig> {
    let Some(account) = own_account else {
        return base;
    };
    let mut runtime = base.unwrap_or_default();
    if runtime.account_id != account {
        // The backend resolves the account's folder again.
        runtime.config_dir = None;
    }
    runtime.account_id = account;
    Some(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, enabled: bool, health: &str) -> ProviderDescriptor {
        let health = match health {
            "unhealthy" => serde_json::json!({ "Unhealthy": { "reason": "sem login" } }),
            _ => serde_json::json!("Healthy"),
        };
        let mut provider: ProviderDescriptor = serde_json::from_value(serde_json::json!({
            "id": id, "label": id, "description": "",
            "models": [
                { "id": "small", "label": "s", "description": "", "recommended": false, "effortLevels": [] },
                { "id": "big", "label": "b", "description": "", "recommended": true, "effortLevels": [] }
            ],
            "capabilities": {
                "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
                "resumable": true, "experimental": false, "canBeDelegationTarget": true,
                "canRequestDelegation": true, "supportsReadOnlyDelegation": true, "supportsEditDelegation": true
            },
            "health": health, "stable": true
        }))
        .expect("descriptor");
        provider.enabled = enabled;
        provider
    }

    #[test]
    fn its_own_provider_and_model_come_first() {
        let providers = [
            provider("claude_code", true, "healthy"),
            provider("codex", true, "unhealthy"),
        ];
        let pick = |agent: Option<&str>, model: Option<&str>, picked| {
            agent_run_pick(agent, model, &providers, picked)
        };
        assert_eq!(
            pick(Some("claude_code"), Some("small"), (Some("codex"), None)),
            Ok(AgentRunPick {
                provider_id: "claude_code".into(),
                model: Some("small".into())
            })
        );
        // A model the provider does not list: its recommended one.
        assert_eq!(
            pick(Some("claude_code"), Some("gone"), (None, None))
                .unwrap()
                .model
                .as_deref(),
            Some("big")
        );
        // Its own provider, even unhealthy, is tried.
        assert!(pick(Some("codex"), None, (None, None)).is_ok());
        // No provider of its own: the picked one, with the picked model.
        assert_eq!(
            pick(None, None, (Some("claude_code"), Some("small")))
                .unwrap()
                .model
                .as_deref(),
            Some("small")
        );
        assert_eq!(
            pick(None, None, (Some("codex"), None)),
            Err(AgentRunError::Unhealthy("sem login".into()))
        );
        assert_eq!(
            pick(None, None, (None, None)),
            Err(AgentRunError::Unavailable)
        );
        let off = [provider("claude_code", false, "healthy")];
        assert_eq!(
            agent_run_pick(Some("claude_code"), None, &off, (None, None)),
            Err(AgentRunError::Unavailable)
        );
    }

    #[test]
    fn effort_and_account_follow_its_own_provider() {
        assert_eq!(
            agent_run_effort(Some("codex"), Some("high")).as_deref(),
            Some("high")
        );
        assert_eq!(agent_run_effort(None, Some("high")), None);
        let base = ProviderRuntimeConfig {
            binary_path: Some("/bin/claude".into()),
            account_id: Some("pessoal".into()),
            config_dir: Some("/accounts/pessoal".into()),
            ..Default::default()
        };
        assert_eq!(
            agent_run_runtime(Some(base.clone()), None),
            Some(base.clone())
        );
        let zedy = agent_run_runtime(Some(base.clone()), Some(Some("zedy".into()))).unwrap();
        assert_eq!(zedy.account_id.as_deref(), Some("zedy"));
        assert_eq!(zedy.config_dir, None);
        assert_eq!(zedy.binary_path.as_deref(), Some("/bin/claude"));
        let same = agent_run_runtime(Some(base.clone()), Some(Some("pessoal".into()))).unwrap();
        assert_eq!(same, base);
        let login = agent_run_runtime(None, Some(None)).unwrap();
        assert_eq!(login, ProviderRuntimeConfig::default());
    }
}
