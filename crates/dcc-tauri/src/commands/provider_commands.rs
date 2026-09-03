use serde::{Deserialize, Serialize};
use specta::Type;
use std::{collections::HashSet, path::Path};
use tauri::State;

use dcc_core::{
    domain::{
        provider::{ProviderAccountUsage, ProviderCatalog, ProviderModelDescriptor},
        session::SessionId,
        workspace::WorkspaceId,
    },
    ports::{ProviderRuntimeConfig, SessionConfig},
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallAntigravityOutput {
    pub binary_path: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectAntigravityInput {
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConnectAntigravityOutput {
    pub models: Vec<ProviderModelDescriptor>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityStatusInput {
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityStatusOutput {
    pub managed_runtime_installed: bool,
    pub runtime_version: Option<String>,
    pub signed_in: bool,
    pub cached_model_count: usize,
    pub last_verified_at: Option<String>,
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
    apply_antigravity_account_overlay(&mut catalog, state);
    apply_provider_availability_overlay(&mut catalog, state)?;
    for descriptor in &catalog.providers {
        if descriptor.capabilities.supports_dynamic_models {
            // The catalog just consulted the runtime; reuse that list as the
            // model authority instead of spawning it again on the next turn.
            state.seed_dynamic_models(
                &descriptor.id.0,
                descriptor
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect(),
            );
        }
    }
    Ok(ListProvidersOutput { catalog })
}

fn apply_antigravity_account_overlay(catalog: &mut ProviderCatalog, state: &SessionCommandState) {
    let Some(account) = crate::antigravity_account_state::load(state.app_data_dir()) else {
        return;
    };
    if !crate::antigravity_account_state::has_saved_login(&account, &account.profile_path) {
        return;
    }
    let Some(descriptor) = catalog
        .providers
        .iter_mut()
        .find(|descriptor| descriptor.id.0 == "antigravity")
    else {
        return;
    };
    let mut seen = descriptor
        .models
        .iter()
        .map(|model| model.id.clone())
        .collect::<HashSet<_>>();
    descriptor.models.extend(
        account
            .models
            .into_iter()
            .filter(|model| seen.insert(model.id.clone())),
    );
}

fn apply_provider_availability_overlay(
    catalog: &mut ProviderCatalog,
    state: &SessionCommandState,
) -> Result<(), String> {
    for descriptor in &mut catalog.providers {
        if descriptor.id.0 == "antigravity"
            && crate::antigravity_installation::managed_executable_path(state.app_data_dir())
                .is_some()
        {
            descriptor.health = dcc_core::domain::provider::HealthStatus::Healthy;
        }
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

#[tauri::command]
pub async fn install_antigravity(
    state: State<'_, SessionCommandState>,
) -> Result<InstallAntigravityOutput, String> {
    let app_data_dir = state.app_data_dir().to_path_buf();
    let install_root = app_data_dir.clone();
    let binary_path = tokio::task::spawn_blocking(move || {
        crate::antigravity_installation::install(&install_root)
    })
    .await
    .map_err(|error| format!("Antigravity installer stopped unexpectedly: {error}"))??;
    let validation_profile = app_data_dir
        .join("tools")
        .join(format!("antigravity-validation-{}", uuid::Uuid::new_v4()));
    let validation = dcc_providers::antigravity_acp::validate_official_runtime(
        &binary_path,
        &validation_profile,
        crate::antigravity_installation::RELEASE_VERSION,
    )
    .await
    .map_err(|error| error.to_string());
    let _ = std::fs::remove_dir_all(&validation_profile);
    validation?;
    crate::antigravity_installation::mark_verified(&binary_path)?;
    Ok(InstallAntigravityOutput {
        binary_path: binary_path.display().to_string(),
        version: crate::antigravity_installation::RELEASE_VERSION.to_string(),
    })
}

#[tauri::command]
pub async fn get_antigravity_status(
    state: State<'_, SessionCommandState>,
    input: AntigravityStatusInput,
) -> Result<AntigravityStatusOutput, String> {
    let runtime = state
        .provider_runtime_config("antigravity", input.provider_runtime.as_ref())
        .map_err(|error| error.to_string())?;
    let profile = runtime
        .home_path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| "Antigravity requires an isolated runtime home".to_string())?;
    let account = crate::antigravity_account_state::load(state.app_data_dir());
    let signed_in = account
        .as_ref()
        .map(|account| crate::antigravity_account_state::has_saved_login(account, profile))
        .unwrap_or_else(|| crate::antigravity_account_state::profile_has_login(profile));
    let cached_model_count = account
        .as_ref()
        .filter(|account| crate::antigravity_account_state::has_saved_login(account, profile))
        .map_or(0, |account| account.models.len());
    let last_verified_at = account
        .filter(|account| crate::antigravity_account_state::has_saved_login(account, profile))
        .map(|account| account.verified_at);
    let managed_runtime_installed =
        crate::antigravity_installation::managed_executable_path(state.app_data_dir()).is_some();
    Ok(AntigravityStatusOutput {
        managed_runtime_installed,
        runtime_version: managed_runtime_installed
            .then(|| crate::antigravity_installation::RELEASE_VERSION.to_string()),
        signed_in,
        cached_model_count,
        last_verified_at,
    })
}

#[tauri::command]
pub async fn connect_antigravity(
    state: State<'_, SessionCommandState>,
    input: ConnectAntigravityInput,
) -> Result<ConnectAntigravityOutput, String> {
    let runtime = state
        .provider_runtime_config("antigravity", input.provider_runtime.as_ref())
        .map_err(|error| error.to_string())?;
    let profile_path = runtime
        .home_path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| "Antigravity requires an isolated runtime home".to_string())?
        .to_path_buf();
    let workspace = state.app_data_dir().join("antigravity-setup-workspace");
    std::fs::create_dir_all(&workspace)
        .map_err(|error| format!("could not prepare Antigravity sign-in: {error}"))?;
    let adapter = dcc_providers::antigravity_acp::AntigravityAcpAdapter::new();
    let models = adapter
        .authenticate_account(SessionConfig {
            workspace_id: WorkspaceId("antigravity-setup".to_string()),
            session_id: SessionId(format!("antigravity-setup-{}", uuid::Uuid::new_v4())),
            model: None,
            working_directory: Some(workspace.display().to_string()),
            additional_working_directories: Vec::new(),
            provider_runtime: Some(runtime),
            mcp_servers: Vec::new(),
        })
        .await
        .map_err(|error| error.to_string())?;
    crate::antigravity_account_state::save(state.app_data_dir(), &profile_path, models.clone())?;
    Ok(ConnectAntigravityOutput { models })
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

    #[test]
    fn account_overlay_restores_antigravity_models_after_backend_restart() {
        let root = tempfile::tempdir().expect("provider command root");
        let root = std::fs::canonicalize(root.path()).expect("physical provider command root");
        let app_data = root.join("app-data");
        let state = SessionCommandState::new_headless(root.join("state.sqlite"), app_data.clone());
        let profile = app_data.join("provider-homes/antigravity");
        std::fs::create_dir_all(profile.join("antigravity-acp")).expect("profile");
        std::fs::write(
            profile.join("antigravity-acp/acp_token.json"),
            b"saved-by-official-runtime",
        )
        .expect("token");
        crate::antigravity_account_state::save(
            &app_data,
            &profile,
            vec![ProviderModelDescriptor {
                id: "gemini-account-model".to_string(),
                label: "Account model".to_string(),
                description: String::new(),
                recommended: false,
                effort_levels: Vec::new(),
            }],
        )
        .expect("save account state");
        let mut catalog = ProviderCatalog {
            providers: vec![dcc_providers::antigravity_acp::descriptor(
                HealthStatus::Healthy,
                Vec::new(),
            )],
        };

        apply_antigravity_account_overlay(&mut catalog, &state);

        assert!(catalog.providers[0]
            .models
            .iter()
            .any(|model| model.id == "gemini-account-model"));
    }
}
