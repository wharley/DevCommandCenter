use serde::{Deserialize, Serialize};
use specta::Type;
use std::{collections::HashSet, path::Path};
use tauri::State;

use dcc_core::{
    domain::{
        provider::{
            ProviderAccount, ProviderAccountStatus, ProviderAccountUsage, ProviderCatalog,
            ProviderModelDescriptor, ProviderResetOutcome,
        },
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
pub struct ProviderAccountResetInput {
    pub provider_id: String,
    #[serde(default)]
    pub credit_id: Option<String>,
    #[serde(default)]
    pub provider_runtime: Option<ProviderRuntimeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountResetOutput {
    pub outcome: ProviderResetOutcome,
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListProviderAccountsOutput {
    pub accounts: Vec<ProviderAccount>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderAccountInput {
    pub provider_id: String,
    pub label: String,
    /// An existing config folder to reference instead of a new one.
    #[serde(default)]
    pub import_dir: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameProviderAccountInput {
    pub account_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountIdInput {
    pub account_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountOutput {
    pub account: ProviderAccount,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountStatusInput {
    pub provider_id: String,
    /// `None` checks the system login.
    #[serde(default)]
    pub account_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountStatusOutput {
    pub status: Option<ProviderAccountStatus>,
    /// What to run in a terminal to sign this account in.
    pub login_command: Option<String>,
}

#[tauri::command]
pub async fn list_provider_accounts(
    state: State<'_, SessionCommandState>,
) -> Result<ListProviderAccountsOutput, String> {
    Ok(ListProviderAccountsOutput {
        accounts: state
            .list_provider_accounts()
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn create_provider_account(
    state: State<'_, SessionCommandState>,
    input: CreateProviderAccountInput,
) -> Result<ProviderAccountOutput, String> {
    Ok(ProviderAccountOutput {
        account: state
            .create_provider_account(
                input.provider_id.trim(),
                &input.label,
                input.import_dir.as_deref(),
            )
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn rename_provider_account(
    state: State<'_, SessionCommandState>,
    input: RenameProviderAccountInput,
) -> Result<ProviderAccountOutput, String> {
    Ok(ProviderAccountOutput {
        account: state
            .rename_provider_account(input.account_id.trim(), &input.label)
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub async fn remove_provider_account(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountIdInput,
) -> Result<(), String> {
    state
        .remove_provider_account(input.account_id.trim())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn provider_account_status(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountStatusOutput, String> {
    provider_account_status_for_state(&state, input).await
}

/// Asks the runtime who it is signed in as, under the account's own config.
pub async fn provider_account_status_for_state(
    state: &SessionCommandState,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountStatusOutput, String> {
    let provider_id = input.provider_id.trim();
    let registration = state
        .require_provider_available(provider_id)
        .map_err(|error| error.to_string())?;
    if !supports_provider_capability(&registration.capabilities, ProviderCapability::Accounts) {
        return Err(format!(
            "provider {provider_id} does not support separate accounts"
        ));
    }
    let runtime = state
        .provider_runtime_config(
            provider_id,
            Some(&ProviderRuntimeConfig {
                account_id: input.account_id.clone(),
                ..ProviderRuntimeConfig::default()
            }),
        )
        .map_err(|error| error.to_string())?;
    let login_command = runtime
        .config_dir
        .as_deref()
        .map(dcc_providers::claude_code::claude_account_login_command)
        .or_else(|| Some("claude auth login".to_string()));
    let status = registration
        .runtime
        .account_status(Some(&runtime))
        .await
        .map_err(|error| error.to_string())?;
    Ok(ProviderAccountStatusOutput {
        status,
        login_command,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAccountSignInOutput {
    pub status: ProviderAccountStatus,
}

/// Sign-ins waiting on the browser, by account, so the person can cancel one
/// (or start it again, which cancels the earlier one).
static ACCOUNT_SIGN_INS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, (u64, tokio::sync::oneshot::Sender<()>)>>,
> = std::sync::OnceLock::new();
static NEXT_ACCOUNT_SIGN_IN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn account_sign_in_key(provider_id: &str, account_id: Option<&str>) -> String {
    format!("{provider_id}\n{}", account_id.unwrap_or_default())
}

fn account_sign_ins(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (u64, tokio::sync::oneshot::Sender<()>)>>
{
    ACCOUNT_SIGN_INS.get_or_init(Default::default)
}

#[tauri::command]
pub async fn sign_in_provider_account(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountSignInOutput, String> {
    sign_in_provider_account_for_state(&state, input).await
}

/// Signs an account (or the system login) in through the browser. Returns
/// once the person authorized, cancelled, or the sign-in gave up.
pub async fn sign_in_provider_account_for_state(
    state: &SessionCommandState,
    input: ProviderAccountStatusInput,
) -> Result<ProviderAccountSignInOutput, String> {
    let provider_id = input.provider_id.trim();
    let registration = state
        .require_provider_available(provider_id)
        .map_err(|error| error.to_string())?;
    if !supports_provider_capability(&registration.capabilities, ProviderCapability::Accounts) {
        return Err(format!(
            "provider {provider_id} does not support separate accounts"
        ));
    }
    let runtime = state
        .provider_runtime_config(
            provider_id,
            Some(&ProviderRuntimeConfig {
                account_id: input.account_id.clone(),
                ..ProviderRuntimeConfig::default()
            }),
        )
        .map_err(|error| error.to_string())?;
    let scratch_dir = state
        .app_data_dir()
        .join("tmp")
        .join(format!("provider-sign-in-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&scratch_dir)
        .map_err(|error| format!("could not prepare the sign-in: {error}"))?;

    let key = account_sign_in_key(provider_id, input.account_id.as_deref());
    let token = NEXT_ACCOUNT_SIGN_IN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    if let Some((_, earlier)) = account_sign_ins()
        .lock()
        .map_err(|error| error.to_string())?
        .insert(key.clone(), (token, cancel))
    {
        let _ = earlier.send(());
    }
    // Dropping the sign-in future on cancel also stops its Claude process.
    let outcome = tokio::select! {
        outcome = registration.runtime.account_sign_in(Some(&runtime), &scratch_dir) => Some(outcome),
        _ = cancelled => None,
    };
    if let Ok(mut sign_ins) = account_sign_ins().lock() {
        if sign_ins.get(&key).is_some_and(|(current, _)| *current == token) {
            sign_ins.remove(&key);
        }
    }
    let _ = std::fs::remove_dir_all(&scratch_dir);
    match outcome {
        Some(Ok(status)) => Ok(ProviderAccountSignInOutput { status }),
        Some(Err(error)) => Err(error.to_string()),
        None => Err("sign-in cancelled".to_string()),
    }
}

#[tauri::command]
pub async fn cancel_provider_account_sign_in(
    input: ProviderAccountStatusInput,
) -> Result<(), String> {
    let key = account_sign_in_key(input.provider_id.trim(), input.account_id.as_deref());
    if let Some((_, cancel)) = account_sign_ins()
        .lock()
        .map_err(|error| error.to_string())?
        .remove(&key)
    {
        let _ = cancel.send(());
    }
    Ok(())
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
            native_resume_id: None,
            native_rewind_checkpoint: None,
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

pub async fn provider_account_reset_for_state(
    state: &SessionCommandState,
    input: ProviderAccountResetInput,
) -> Result<ProviderAccountResetOutput, String> {
    let provider_id = input.provider_id.trim();
    let registration = state
        .require_provider_available(provider_id)
        .map_err(|error| error.to_string())?;
    if !supports_provider_capability(
        &registration.capabilities,
        ProviderCapability::AccountResets,
    ) {
        return Err(format!(
            "provider {provider_id} does not support account usage resets"
        ));
    }
    let runtime = input
        .provider_runtime
        .as_ref()
        .map(|runtime| state.provider_runtime_config(provider_id, Some(runtime)))
        .transpose()
        .map_err(|error| error.to_string())?;
    let outcome = registration
        .runtime
        .consume_account_reset(runtime.as_ref(), input.credit_id.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    Ok(ProviderAccountResetOutput { outcome })
}

#[tauri::command]
pub async fn provider_account_reset(
    state: State<'_, SessionCommandState>,
    input: ProviderAccountResetInput,
) -> Result<ProviderAccountResetOutput, String> {
    provider_account_reset_for_state(&state, input).await
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
