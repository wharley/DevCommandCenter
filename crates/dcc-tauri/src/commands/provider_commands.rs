use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_core::{
    domain::provider::{ProviderAccountUsage, ProviderCatalog},
    ports::ProviderRuntimeConfig,
};

use dcc_providers::{supports_provider_capability, ProviderCapability};

use crate::state::{ProviderAvailability, SessionCommandState};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListProvidersOutput {
    pub catalog: ProviderCatalog,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageInput {
    pub provider_id: String,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountUsageOutput {
    pub usage: Option<ProviderAccountUsage>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAvailabilityInput {
    pub provider_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetProviderAvailabilityInput {
    pub provider_id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAvailabilityOutput {
    pub availability: ProviderAvailability,
}

#[tauri::command]
pub async fn list_providers(
    state: State<'_, SessionCommandState>,
) -> Result<ListProvidersOutput, String> {
    list_providers_for_state(&state).await
}

pub async fn list_providers_for_state(
    state: &SessionCommandState,
) -> Result<ListProvidersOutput, String> {
    let mut catalog = dcc_providers::provider_catalog().await;
    apply_provider_availability_overlay(&mut catalog, state)?;
    Ok(ListProvidersOutput { catalog })
}

fn apply_provider_availability_overlay(
    catalog: &mut ProviderCatalog,
    state: &SessionCommandState,
) -> Result<(), String> {
    for descriptor in &mut catalog.providers {
        let availability = state
            .provider_availability(&descriptor.id.0)
            .map_err(|error| error.to_string())?;
        descriptor.enabled = availability.enabled;
        descriptor.availability_generation = availability.generation;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_provider_availability(
    state: State<'_, SessionCommandState>,
    input: ProviderAvailabilityInput,
) -> Result<ProviderAvailabilityOutput, String> {
    Ok(ProviderAvailabilityOutput {
        availability: state
            .provider_availability(input.provider_id.trim())
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn set_provider_availability(
    state: State<'_, SessionCommandState>,
    input: SetProviderAvailabilityInput,
) -> Result<ProviderAvailabilityOutput, String> {
    Ok(ProviderAvailabilityOutput {
        availability: state
            .set_provider_enabled(input.provider_id.trim(), input.enabled)
            .await
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn provider_account_usage(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    provider_account_usage_for_state(&state, input).await
}

/// Account usage is gated by the registered capability and by server-backed
/// availability before any adapter code runs: a provider without the
/// capability, or a disabled one, must never spawn a runtime to answer.
pub async fn provider_account_usage_for_state(
    state: &SessionCommandState,
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    let provider_id = input.provider_id.trim();
    let registration = state
        .require_provider_available(provider_id)
        .map_err(|error| error.to_string())?;
    if !supports_provider_capability(&registration.capabilities, ProviderCapability::AccountUsage) {
        return Err(format!(
            "provider {provider_id} does not support account usage"
        ));
    }
    let runtime = input
        .provider_runtime
        .as_ref()
        .map(|runtime| state.provider_runtime_config(provider_id, Some(runtime)))
        .transpose()
        .map_err(|error| error.to_string())?;
    let usage = registration
        .runtime
        .account_usage(runtime.as_ref())
        .await
        .map_err(|error| error.to_string())?;
    Ok(ProviderAccountUsageOutput { usage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::provider::HealthStatus;

    #[tokio::test(flavor = "current_thread")]
    async fn account_usage_is_refused_before_any_runtime_for_unsupported_or_disabled_providers() {
        let root = tempfile::tempdir().expect("provider command root");
        let root = std::fs::canonicalize(root.path()).expect("physical provider command root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));

        let unsupported = provider_account_usage_for_state(
            &state,
            ProviderAccountUsageInput {
                provider_id: "droid".to_string(),
                provider_runtime: None,
            },
        )
        .await
        .expect_err("droid has no usage capability");
        assert!(unsupported.contains("does not support account usage"));

        let unknown = provider_account_usage_for_state(
            &state,
            ProviderAccountUsageInput {
                provider_id: "unknown-provider".to_string(),
                provider_runtime: None,
            },
        )
        .await
        .expect_err("unknown provider");
        assert!(unknown.contains("unknown provider runtime"));

        state
            .set_provider_enabled("codex", false)
            .await
            .expect("disable codex");
        let disabled = provider_account_usage_for_state(
            &state,
            ProviderAccountUsageInput {
                provider_id: "codex".to_string(),
                provider_runtime: None,
            },
        )
        .await
        .expect_err("disabled codex must not spawn a runtime for usage");
        assert!(disabled.contains("is disabled"));

        state
            .set_provider_enabled("codex", true)
            .await
            .expect("enable codex");
        let invalid_runtime = provider_account_usage_for_state(
            &state,
            ProviderAccountUsageInput {
                provider_id: "claude_code".to_string(),
                provider_runtime: Some(ProviderRuntimeConfig {
                    max_concurrent_subagents: Some(2),
                    ..ProviderRuntimeConfig::default()
                }),
            },
        )
        .await
        .expect_err("claude ignores subagent limits");
        assert!(invalid_runtime.contains("does not support subagent concurrency limits"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn availability_overlay_preserves_runtime_health_models_and_capabilities() {
        let root = tempfile::tempdir().expect("provider command root");
        let root = std::fs::canonicalize(root.path()).expect("physical provider command root");
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        state
            .set_provider_enabled("droid", false)
            .await
            .expect("disable droid");
        let descriptor = dcc_providers::droid::descriptor(HealthStatus::Unhealthy {
            reason: "fixture".to_string(),
        });
        let expected_models = descriptor.models.clone();
        let expected_capabilities = descriptor.capabilities.clone();
        let expected_health = descriptor.health.clone();
        let mut catalog = ProviderCatalog {
            providers: vec![descriptor],
        };

        apply_provider_availability_overlay(&mut catalog, &state).expect("availability overlay");
        let projected = &catalog.providers[0];
        assert!(!projected.enabled);
        assert_eq!(projected.availability_generation, 1);
        assert_eq!(
            serde_json::to_value(&projected.models).expect("models json"),
            serde_json::to_value(expected_models).expect("expected models json")
        );
        assert_eq!(
            serde_json::to_value(&projected.capabilities).expect("capabilities json"),
            serde_json::to_value(expected_capabilities).expect("expected capabilities json")
        );
        assert_eq!(
            serde_json::to_value(&projected.health).expect("health json"),
            serde_json::to_value(expected_health).expect("expected health json")
        );
    }
}
