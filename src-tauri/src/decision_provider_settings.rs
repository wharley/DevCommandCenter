//! UI-backed configuration for the optional Decision Provider.
//!
//! Public settings are kept in the application data directory. The TypeSafe
//! API key is kept in the OS credential store and is only materialised in the
//! current DCC process when the provider is enabled.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use dcc_core::{
    domain::mcp::McpSecretReferenceId,
    ports::{CredentialStore, SecretValue},
};
use dcc_infra::credential_store::SystemCredentialStore;

const SETTINGS_FILE_NAME: &str = "decision-provider-settings.json";
const API_KEY_REFERENCE: &str = "decision-provider:typesafe";
const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
const DEFAULT_MODEL: &str = "jev-latest";
const DEFAULT_THRESHOLD: f64 = 0.65;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionProviderSettingsInput {
    pub provider: String,
    pub mode: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub memory_threshold: Option<f64>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionProviderSettingsOutput {
    pub provider: String,
    pub mode: String,
    pub base_url: String,
    pub model: String,
    pub memory_threshold: f64,
    pub api_key_configured: bool,
    pub restart_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDecisionProviderSettings {
    provider: String,
    mode: String,
    base_url: String,
    model: String,
    memory_threshold: f64,
}

pub struct DecisionProviderSettings;

impl DecisionProviderSettings {
    pub async fn load_persisted_settings(app_data_dir: &Path) -> Result<(), String> {
        let path = app_data_dir.join(SETTINGS_FILE_NAME);
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(format!(
                    "could not read decision provider settings: {error}"
                ))
            }
        };
        let settings: PersistedDecisionProviderSettings = serde_json::from_str(&contents)
            .map_err(|error| format!("could not parse decision provider settings: {error}"))?;
        validate_persisted(&settings)?;
        apply_settings_to_environment(&settings);
        if settings.provider == "typesafe" {
            Self::load_api_key_into_process().await
        } else {
            std::env::remove_var("TYPESAFE_API_KEY");
            Ok(())
        }
    }

    pub async fn read_settings(
        app_data_dir: &Path,
    ) -> Result<DecisionProviderSettingsOutput, String> {
        let path = app_data_dir.join(SETTINGS_FILE_NAME);
        let settings = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str::<PersistedDecisionProviderSettings>(&contents)
                .map_err(|error| format!("could not parse decision provider settings: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                default_settings_from_environment()
            }
            Err(error) => {
                return Err(format!(
                    "could not read decision provider settings: {error}"
                ))
            }
        };
        validate_persisted(&settings)?;
        let api_key_configured = Self::api_key_configured().await?;
        Ok(output_for(&settings, api_key_configured, false))
    }

    pub async fn save_settings(
        app_data_dir: &Path,
        input: DecisionProviderSettingsInput,
    ) -> Result<DecisionProviderSettingsOutput, String> {
        validate_input(&input)?;
        let persisted = PersistedDecisionProviderSettings {
            provider: input.provider.trim().to_ascii_lowercase(),
            mode: input.mode.trim().to_ascii_lowercase(),
            base_url: input
                .base_url
                .as_deref()
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            model: input
                .model
                .as_deref()
                .unwrap_or(DEFAULT_MODEL)
                .trim()
                .to_string(),
            memory_threshold: input.memory_threshold.unwrap_or(DEFAULT_THRESHOLD),
        };
        let bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|error| format!("could not encode decision provider settings: {error}"))?;
        fs::write(app_data_dir.join(SETTINGS_FILE_NAME), bytes)
            .map_err(|error| format!("could not save decision provider settings: {error}"))?;

        let store = SystemCredentialStore::default();
        let reference = McpSecretReferenceId(API_KEY_REFERENCE.to_string());
        if let Some(api_key) = input.api_key.as_deref().map(str::trim) {
            if api_key.is_empty() {
                store
                    .delete_secret(&reference)
                    .await
                    .map_err(|error| format!("could not clear TypeSafe API key: {error}"))?;
                std::env::remove_var("TYPESAFE_API_KEY");
            } else {
                store
                    .store_secret(
                        &reference,
                        SecretValue::new(api_key.as_bytes().to_vec())
                            .map_err(|error| error.to_string())?,
                    )
                    .await
                    .map_err(|error| format!("could not save TypeSafe API key: {error}"))?;
                std::env::set_var("TYPESAFE_API_KEY", api_key);
            }
        } else {
            if persisted.provider == "typesafe" {
                Self::load_api_key_into_process().await?;
            } else {
                std::env::remove_var("TYPESAFE_API_KEY");
            }
        }

        apply_settings_to_environment(&persisted);
        if persisted.provider != "typesafe" {
            std::env::remove_var("TYPESAFE_API_KEY");
        }
        let api_key_configured = Self::api_key_configured().await?;
        Ok(output_for(&persisted, api_key_configured, false))
    }

    async fn api_key_configured() -> Result<bool, String> {
        if std::env::var("TYPESAFE_API_KEY")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Ok(true);
        }
        Ok(SystemCredentialStore::default()
            .resolve_secret(&McpSecretReferenceId(API_KEY_REFERENCE.to_string()))
            .await
            .map_err(|error| format!("could not read TypeSafe API key: {error}"))?
            .is_some())
    }

    async fn load_api_key_into_process() -> Result<(), String> {
        let key = SystemCredentialStore::default()
            .resolve_secret(&McpSecretReferenceId(API_KEY_REFERENCE.to_string()))
            .await
            .map_err(|error| format!("could not read TypeSafe API key: {error}"))?;
        match key {
            Some(key) => {
                let key = String::from_utf8(key.expose_secret().to_vec())
                    .map_err(|_| "stored TypeSafe API key is not valid UTF-8".to_string())?;
                std::env::set_var("TYPESAFE_API_KEY", key);
            }
            None => std::env::remove_var("TYPESAFE_API_KEY"),
        }
        Ok(())
    }
}

fn default_settings_from_environment() -> PersistedDecisionProviderSettings {
    PersistedDecisionProviderSettings {
        provider: std::env::var("DCC_DECISION_PROVIDER")
            .unwrap_or_else(|_| "disabled".to_string())
            .trim()
            .to_ascii_lowercase(),
        mode: std::env::var("DCC_DECISION_MODE")
            .unwrap_or_else(|_| "observe".to_string())
            .trim()
            .to_ascii_lowercase(),
        base_url: std::env::var("TYPESAFE_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string(),
        model: std::env::var("TYPESAFE_MODEL")
            .unwrap_or_else(|_| DEFAULT_MODEL.to_string())
            .trim()
            .to_string(),
        memory_threshold: std::env::var("DCC_DECISION_MEMORY_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_THRESHOLD),
    }
}

fn output_for(
    settings: &PersistedDecisionProviderSettings,
    api_key_configured: bool,
    restart_required: bool,
) -> DecisionProviderSettingsOutput {
    DecisionProviderSettingsOutput {
        provider: settings.provider.clone(),
        mode: settings.mode.clone(),
        base_url: settings.base_url.clone(),
        model: settings.model.clone(),
        memory_threshold: settings.memory_threshold,
        api_key_configured,
        restart_required,
    }
}

fn validate_input(input: &DecisionProviderSettingsInput) -> Result<(), String> {
    if !matches!(
        input.provider.trim().to_ascii_lowercase().as_str(),
        "disabled" | "typesafe"
    ) {
        return Err("decision provider must be disabled or typesafe".to_string());
    }
    if !matches!(
        input.mode.trim().to_ascii_lowercase().as_str(),
        "observe" | "enforce"
    ) {
        return Err("decision provider mode must be observe or enforce".to_string());
    }
    if let Some(base_url) = input
        .base_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err("decision provider URL must use http:// or https://".to_string());
        }
        if base_url.chars().count() > 500 || base_url.contains('\0') {
            return Err("decision provider URL is invalid".to_string());
        }
    }
    if let Some(model) = input
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if model.chars().count() > 200 || model.contains('\0') {
            return Err("decision provider model is invalid".to_string());
        }
    }
    if let Some(threshold) = input.memory_threshold {
        if !(0.0..=1.0).contains(&threshold) || !threshold.is_finite() {
            return Err("memory relevance threshold must be between 0 and 1".to_string());
        }
    }
    if let Some(api_key) = input.api_key.as_deref() {
        if api_key.chars().count() > 500 || api_key.contains('\0') {
            return Err("TypeSafe API key is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_persisted(settings: &PersistedDecisionProviderSettings) -> Result<(), String> {
    validate_input(&DecisionProviderSettingsInput {
        provider: settings.provider.clone(),
        mode: settings.mode.clone(),
        base_url: Some(settings.base_url.clone()),
        model: Some(settings.model.clone()),
        memory_threshold: Some(settings.memory_threshold),
        api_key: None,
    })
}

fn apply_settings_to_environment(settings: &PersistedDecisionProviderSettings) {
    std::env::set_var("DCC_DECISION_MODE", &settings.mode);
    std::env::set_var("TYPESAFE_BASE_URL", &settings.base_url);
    std::env::set_var("TYPESAFE_MODEL", &settings.model);
    std::env::set_var(
        "DCC_DECISION_MEMORY_THRESHOLD",
        settings.memory_threshold.to_string(),
    );
    if settings.provider == "typesafe" {
        std::env::set_var("DCC_DECISION_PROVIDER", "typesafe");
    } else {
        std::env::remove_var("DCC_DECISION_PROVIDER");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_provider_and_mode() {
        let input = DecisionProviderSettingsInput {
            provider: "typesafe".to_string(),
            mode: "observe".to_string(),
            base_url: Some(DEFAULT_BASE_URL.to_string()),
            model: Some(DEFAULT_MODEL.to_string()),
            memory_threshold: Some(DEFAULT_THRESHOLD),
            api_key: None,
        };
        assert!(validate_input(&input).is_ok());
    }

    #[test]
    fn rejects_invalid_threshold() {
        let input = DecisionProviderSettingsInput {
            provider: "typesafe".to_string(),
            mode: "observe".to_string(),
            base_url: None,
            model: None,
            memory_threshold: Some(2.0),
            api_key: None,
        };
        assert!(validate_input(&input).is_err());
    }
}
