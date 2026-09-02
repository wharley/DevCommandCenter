use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_core::{
    domain::provider::{ProviderAccountUsage, ProviderCatalog},
    ports::ProviderRuntimeConfig,
};

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
    input: ProviderAccountUsageInput,
) -> Result<ProviderAccountUsageOutput, String> {
    let Some(provider) = dcc_providers::provider_runtime(&input.provider_id) else {
        return Ok(ProviderAccountUsageOutput { usage: None });
    };
    let usage = provider
        .account_usage(input.provider_runtime.as_ref())
        .await
        .map_err(|error| error.to_string())?;
    Ok(ProviderAccountUsageOutput { usage })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::provider::HealthStatus;

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
