//! UI-backed configuration for the optional Decision Provider.
//!
//! Public settings are kept in the application data directory. The TypeSafe
//! API key is kept in the OS credential store and is only materialised in the
//! current DCC process when the provider is enabled.

use std::path::Path;
use std::{env, fs};

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
const DEFAULT_MEMORY_THRESHOLD: f64 = 0.65;
const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.65;
const DEFAULT_COMPLETENESS_THRESHOLD: f64 = 0.80;
const DEFAULT_MODEL_THRESHOLD: f64 = 0.80;

fn default_model_threshold() -> f64 {
    DEFAULT_MODEL_THRESHOLD
}

fn default_true() -> bool {
    true
}

fn default_confidence_threshold() -> f64 {
    DEFAULT_CONFIDENCE_THRESHOLD
}

fn default_completeness_threshold() -> f64 {
    DEFAULT_COMPLETENESS_THRESHOLD
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn default_model_routing() -> String {
    "manual".to_string()
}

fn normalize_model_routing(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "automatic" => "automatic".to_string(),
        _ => "manual".to_string(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionProviderSettingsInput {
    pub provider: String,
    pub mode: String,
    pub model_routing: String,
    #[serde(default = "default_true")]
    pub memory_filter_enabled: bool,
    #[serde(default = "default_true")]
    pub skill_router_enabled: bool,
    #[serde(default = "default_true")]
    pub model_router_enabled: bool,
    #[serde(default = "default_true")]
    pub completion_review_enabled: bool,
    #[serde(default = "default_true")]
    pub tool_guard_enabled: bool,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub memory_threshold: Option<f64>,
    pub skill_confidence_threshold: Option<f64>,
    pub model_confidence_threshold: Option<f64>,
    pub completion_threshold: Option<f64>,
    pub tool_risk_threshold: Option<f64>,
    pub api_key: Option<String>,
    #[serde(default)]
    pub clear_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionProviderSettingsOutput {
    pub provider: String,
    pub mode: String,
    pub model_routing: String,
    pub memory_filter_enabled: bool,
    pub skill_router_enabled: bool,
    pub model_router_enabled: bool,
    pub completion_review_enabled: bool,
    #[serde(default = "default_true")]
    pub tool_guard_enabled: bool,
    pub base_url: String,
    pub model: String,
    pub memory_threshold: f64,
    pub skill_confidence_threshold: f64,
    pub model_confidence_threshold: f64,
    pub completion_threshold: f64,
    pub tool_risk_threshold: f64,
    pub api_key_configured: bool,
    pub restart_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDecisionProviderSettings {
    provider: String,
    mode: String,
    #[serde(default = "default_model_routing")]
    model_routing: String,
    #[serde(default = "default_true")]
    memory_filter_enabled: bool,
    #[serde(default = "default_true")]
    skill_router_enabled: bool,
    #[serde(default = "default_true")]
    model_router_enabled: bool,
    #[serde(default = "default_true")]
    completion_review_enabled: bool,
    #[serde(default = "default_true")]
    tool_guard_enabled: bool,
    base_url: String,
    model: String,
    memory_threshold: f64,
    #[serde(default = "default_confidence_threshold")]
    skill_confidence_threshold: f64,
    #[serde(default = "default_model_threshold")]
    model_confidence_threshold: f64,
    #[serde(default = "default_completeness_threshold")]
    completion_threshold: f64,
    #[serde(default = "default_confidence_threshold")]
    tool_risk_threshold: f64,
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
            model_routing: normalize_model_routing(&input.model_routing),
            memory_filter_enabled: input.memory_filter_enabled,
            skill_router_enabled: input.skill_router_enabled,
            model_router_enabled: input.model_router_enabled,
            completion_review_enabled: input.completion_review_enabled,
            tool_guard_enabled: input.tool_guard_enabled,
            tool_risk_threshold: input
                .tool_risk_threshold
                .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD),
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
            memory_threshold: input.memory_threshold.unwrap_or(DEFAULT_MEMORY_THRESHOLD),
            skill_confidence_threshold: input
                .skill_confidence_threshold
                .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD),
            model_confidence_threshold: input
                .model_confidence_threshold
                .unwrap_or(DEFAULT_MODEL_THRESHOLD),
            completion_threshold: input
                .completion_threshold
                .unwrap_or(DEFAULT_COMPLETENESS_THRESHOLD),
        };
        let bytes = serde_json::to_vec_pretty(&persisted)
            .map_err(|error| format!("could not encode decision provider settings: {error}"))?;
        fs::write(app_data_dir.join(SETTINGS_FILE_NAME), bytes)
            .map_err(|error| format!("could not save decision provider settings: {error}"))?;

        let store = SystemCredentialStore::default();
        let reference = McpSecretReferenceId(API_KEY_REFERENCE.to_string());
        if input.clear_api_key {
            store
                .delete_secret(&reference)
                .await
                .map_err(|error| format!("could not clear TypeSafe API key: {error}"))?;
            std::env::remove_var("TYPESAFE_API_KEY");
        } else if let Some(api_key) = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            store
                .store_secret(
                    &reference,
                    SecretValue::new(api_key.as_bytes().to_vec())
                        .map_err(|error| error.to_string())?,
                )
                .await
                .map_err(|error| format!("could not save TypeSafe API key: {error}"))?;
            std::env::set_var("TYPESAFE_API_KEY", api_key);
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
        model_routing: normalize_model_routing(
            &std::env::var("DCC_MODEL_ROUTING_MODE").unwrap_or_else(|_| default_model_routing()),
        ),
        memory_filter_enabled: env_bool("DCC_DECISION_MEMORY_FILTER_ENABLED", true),
        skill_router_enabled: env_bool("DCC_DECISION_SKILL_ROUTER_ENABLED", true),
        model_router_enabled: env_bool("DCC_DECISION_MODEL_ROUTER_ENABLED", true),
        completion_review_enabled: env_bool("DCC_DECISION_COMPLETION_REVIEW_ENABLED", true),
        tool_guard_enabled: env_bool("DCC_DECISION_TOOL_GUARD_ENABLED", true),
        tool_risk_threshold: env::var("DCC_DECISION_TOOL_RISK_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD),
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
            .unwrap_or(DEFAULT_MEMORY_THRESHOLD),
        skill_confidence_threshold: std::env::var("DCC_DECISION_SKILL_CONFIDENCE_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD),
        model_confidence_threshold: std::env::var("DCC_DECISION_MODEL_CONFIDENCE_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_MODEL_THRESHOLD),
        completion_threshold: std::env::var("DCC_DECISION_COMPLETION_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_COMPLETENESS_THRESHOLD),
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
        model_routing: normalize_model_routing(&settings.model_routing),
        memory_filter_enabled: settings.memory_filter_enabled,
        skill_router_enabled: settings.skill_router_enabled,
        model_router_enabled: settings.model_router_enabled,
        completion_review_enabled: settings.completion_review_enabled,
        tool_guard_enabled: settings.tool_guard_enabled,
        base_url: settings.base_url.clone(),
        model: settings.model.clone(),
        memory_threshold: settings.memory_threshold,
        skill_confidence_threshold: settings.skill_confidence_threshold,
        model_confidence_threshold: settings.model_confidence_threshold,
        completion_threshold: settings.completion_threshold,
        tool_risk_threshold: settings.tool_risk_threshold,
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
    if !matches!(
        input.model_routing.trim().to_ascii_lowercase().as_str(),
        "disabled" | "manual" | "automatic"
    ) {
        return Err("model routing must be disabled, manual, or automatic".to_string());
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
    validate_threshold(input.memory_threshold, "memory relevance")?;
    validate_threshold(input.skill_confidence_threshold, "skill confidence")?;
    validate_threshold(input.model_confidence_threshold, "model confidence")?;
    validate_threshold(input.completion_threshold, "completion")?;
    validate_threshold(input.tool_risk_threshold, "tool risk")?;
    if let Some(api_key) = input.api_key.as_deref() {
        if api_key.chars().count() > 500 || api_key.contains('\0') {
            return Err("TypeSafe API key is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_threshold(value: Option<f64>, label: &str) -> Result<(), String> {
    if let Some(threshold) = value {
        if !(0.0..=1.0).contains(&threshold) || !threshold.is_finite() {
            return Err(format!("{label} threshold must be between 0 and 1"));
        }
    }
    Ok(())
}

fn validate_persisted(settings: &PersistedDecisionProviderSettings) -> Result<(), String> {
    validate_input(&DecisionProviderSettingsInput {
        provider: settings.provider.clone(),
        mode: settings.mode.clone(),
        model_routing: settings.model_routing.clone(),
        memory_filter_enabled: settings.memory_filter_enabled,
        skill_router_enabled: settings.skill_router_enabled,
        model_router_enabled: settings.model_router_enabled,
        completion_review_enabled: settings.completion_review_enabled,
        tool_guard_enabled: settings.tool_guard_enabled,
        base_url: Some(settings.base_url.clone()),
        model: Some(settings.model.clone()),
        memory_threshold: Some(settings.memory_threshold),
        skill_confidence_threshold: Some(settings.skill_confidence_threshold),
        model_confidence_threshold: Some(settings.model_confidence_threshold),
        completion_threshold: Some(settings.completion_threshold),
        tool_risk_threshold: Some(settings.tool_risk_threshold),
        api_key: None,
        clear_api_key: false,
    })
}

fn apply_settings_to_environment(settings: &PersistedDecisionProviderSettings) {
    env::set_var(
        "DCC_DECISION_TOOL_GUARD_ENABLED",
        settings.tool_guard_enabled.to_string(),
    );
    env::set_var(
        "DCC_DECISION_TOOL_RISK_THRESHOLD",
        settings.tool_risk_threshold.to_string(),
    );
    std::env::set_var("DCC_DECISION_MODE", &settings.mode);
    std::env::set_var(
        "DCC_MODEL_ROUTING_MODE",
        normalize_model_routing(&settings.model_routing),
    );
    std::env::set_var(
        "DCC_DECISION_MEMORY_FILTER_ENABLED",
        settings.memory_filter_enabled.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_SKILL_ROUTER_ENABLED",
        settings.skill_router_enabled.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_MODEL_ROUTER_ENABLED",
        settings.model_router_enabled.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_COMPLETION_REVIEW_ENABLED",
        settings.completion_review_enabled.to_string(),
    );
    std::env::set_var("TYPESAFE_BASE_URL", &settings.base_url);
    std::env::set_var("TYPESAFE_MODEL", &settings.model);
    std::env::set_var(
        "DCC_DECISION_MEMORY_THRESHOLD",
        settings.memory_threshold.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_SKILL_CONFIDENCE_THRESHOLD",
        settings.skill_confidence_threshold.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_MODEL_CONFIDENCE_THRESHOLD",
        settings.model_confidence_threshold.to_string(),
    );
    std::env::set_var(
        "DCC_DECISION_COMPLETION_THRESHOLD",
        settings.completion_threshold.to_string(),
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
            model_routing: "manual".to_string(),
            memory_filter_enabled: true,
            skill_router_enabled: true,
            model_router_enabled: true,
            completion_review_enabled: true,
            tool_guard_enabled: true,
            tool_risk_threshold: Some(0.65),
            base_url: Some(DEFAULT_BASE_URL.to_string()),
            model: Some(DEFAULT_MODEL.to_string()),
            memory_threshold: Some(DEFAULT_MEMORY_THRESHOLD),
            skill_confidence_threshold: Some(DEFAULT_CONFIDENCE_THRESHOLD),
            model_confidence_threshold: Some(DEFAULT_CONFIDENCE_THRESHOLD),
            completion_threshold: Some(DEFAULT_COMPLETENESS_THRESHOLD),
            api_key: None,
            clear_api_key: false,
        };
        assert!(validate_input(&input).is_ok());
    }

    #[test]
    fn rejects_invalid_threshold() {
        let input = DecisionProviderSettingsInput {
            provider: "typesafe".to_string(),
            mode: "observe".to_string(),
            model_routing: "manual".to_string(),
            memory_filter_enabled: true,
            skill_router_enabled: true,
            model_router_enabled: true,
            completion_review_enabled: true,
            tool_guard_enabled: true,
            tool_risk_threshold: Some(0.65),
            base_url: None,
            model: None,
            memory_threshold: Some(2.0),
            skill_confidence_threshold: Some(DEFAULT_CONFIDENCE_THRESHOLD),
            model_confidence_threshold: Some(DEFAULT_CONFIDENCE_THRESHOLD),
            completion_threshold: Some(DEFAULT_COMPLETENESS_THRESHOLD),
            api_key: None,
            clear_api_key: false,
        };
        assert!(validate_input(&input).is_err());
    }
    #[test]
    fn legacy_settings_preserve_thresholds_and_default_independent_tool_policy() {
        let persisted: PersistedDecisionProviderSettings = serde_json::from_value(serde_json::json!({
            "provider":"typesafe", "mode":"observe", "baseUrl":DEFAULT_BASE_URL, "model":DEFAULT_MODEL,
            "memoryThreshold":0.3, "modelConfidenceThreshold":0.65, "completionThreshold":0.65
        })).unwrap();
        assert_eq!(persisted.model_confidence_threshold, 0.65);
        assert_eq!(persisted.completion_threshold, 0.65);
        assert_eq!(persisted.tool_risk_threshold, 0.65);
        assert!(persisted.tool_guard_enabled);
        assert!(validate_persisted(&persisted).is_ok());
    }

    #[test]
    fn rejects_invalid_tool_threshold_independently() {
        let mut persisted: PersistedDecisionProviderSettings = serde_json::from_value(serde_json::json!({
            "provider":"typesafe", "mode":"observe", "baseUrl":DEFAULT_BASE_URL, "model":DEFAULT_MODEL,
            "memoryThreshold":0.65, "toolRiskThreshold":1.5
        })).unwrap();
        assert!(validate_persisted(&persisted).is_err());
        persisted.tool_risk_threshold = 0.4;
        assert!(validate_persisted(&persisted).is_ok());
        assert_eq!(persisted.model_confidence_threshold, 0.8);
        assert_eq!(persisted.completion_threshold, 0.8);
    }
}
