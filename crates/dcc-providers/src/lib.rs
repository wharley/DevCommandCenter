pub mod antigravity_acp;
pub mod claude_code;
mod claude_mcp;
pub mod claude_sdk_sidecar;
pub mod codex;
pub mod codex_app_server;
mod codex_mcp;
pub mod common;
pub mod cursor;
mod cursor_acp;
mod cursor_mcp;
pub mod droid;
// The Factory JSON-RPC projection remains inert until an exact Droid CLI
// runtime exposes structured MCP server ownership in permission requests.
#[allow(dead_code)]
mod droid_mcp;
pub mod gemini;
// The exact-version ACP projection remains inert until Gemini emits
// structured MCP ownership in permission and tool-call events.
#[allow(dead_code)]
mod gemini_mcp;
pub mod grok_acp;
// The exact-version ACP projection remains inert until the installed Grok
// runtime proves structured MCP ownership in permission and tool-call events.
#[allow(dead_code)]
mod grok_mcp;
pub mod headless_cli;

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use futures::future::join_all;

use dcc_core::domain::provider::{
    Capabilities, HealthStatus, McpSupportLevel, ProviderApprovalPolicy, ProviderCatalog,
    ProviderDescriptor,
};
use dcc_core::ports::Provider;

pub const PROVIDER_IDS: [&str; 7] = [
    "claude_code",
    "codex",
    "cursor",
    "grok",
    "antigravity",
    "gemini",
    "droid",
];

fn provider_registry() -> &'static HashMap<String, Arc<dyn Provider>> {
    static REGISTRY: OnceLock<HashMap<String, Arc<dyn Provider>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let providers: [Arc<dyn Provider>; 7] = [
            Arc::new(claude_code::adapter()),
            Arc::new(codex::adapter()),
            Arc::new(cursor::adapter()),
            Arc::new(grok_acp::GrokAcpAdapter::new(
                grok_acp::stable_grok_capabilities(),
            )),
            Arc::new(antigravity_acp::AntigravityAcpAdapter::new()),
            Arc::new(gemini::adapter()),
            Arc::new(droid::adapter()),
        ];

        let mut registry = HashMap::with_capacity(PROVIDER_IDS.len());
        for provider in providers {
            registry.insert(provider.id().0.clone(), provider);
        }
        registry
    })
}

/// Read-only view of exactly one registered adapter. Consumers use this
/// registration rather than rebuilding provider capability tables in Tauri.
#[derive(Clone)]
pub struct ProviderRegistration {
    pub runtime: Arc<dyn Provider>,
    pub capabilities: Capabilities,
}

/// Capability checks are pure and derived from an adapter's registered runtime
/// contract. They intentionally do not encode health/enabled policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderCapability {
    Steering,
    NativeSubagentSteering,
    NativeSubagentInterrupt,
    ReadOnlyDelegation,
    EditDelegation,
    DelegationTarget,
    DelegationRequest,
    MultiRoot,
    RuntimeHome,
    RuntimeBinary,
    ShadowHome,
    SubagentConcurrency,
    AccountUsage,
    DynamicModels,
    CompactionCommand,
}

pub fn supports_provider_capability(
    capabilities: &Capabilities,
    capability: ProviderCapability,
) -> bool {
    match capability {
        ProviderCapability::Steering => capabilities.supports_steering,
        ProviderCapability::NativeSubagentSteering => {
            capabilities.supports_native_subagent_steering
        }
        ProviderCapability::NativeSubagentInterrupt => {
            capabilities.supports_native_subagent_interrupt
        }
        ProviderCapability::ReadOnlyDelegation => capabilities.supports_read_only_delegation,
        ProviderCapability::EditDelegation => capabilities.supports_edit_delegation,
        ProviderCapability::DelegationTarget => capabilities.can_be_delegation_target,
        ProviderCapability::DelegationRequest => capabilities.can_request_delegation,
        ProviderCapability::MultiRoot => capabilities.supports_multi_root,
        ProviderCapability::RuntimeHome => capabilities.supports_runtime_home,
        ProviderCapability::RuntimeBinary => capabilities.supports_runtime_binary,
        ProviderCapability::ShadowHome => capabilities.supports_shadow_home,
        ProviderCapability::SubagentConcurrency => capabilities.supports_subagent_concurrency,
        ProviderCapability::AccountUsage => capabilities.supports_account_usage,
        ProviderCapability::DynamicModels => capabilities.supports_dynamic_models,
        ProviderCapability::CompactionCommand => capabilities.supports_compaction_command,
    }
}

pub const MAX_MODEL_ID_CHARS: usize = 128;

/// Model authority: a bounded id, and membership in the provider's static
/// catalog unless the adapter discovers models at runtime. Aliases resolve.
pub fn validate_provider_model(
    provider_id: &str,
    capabilities: &Capabilities,
    model: Option<&str>,
) -> Result<(), String> {
    let Some(model) = model else {
        return Ok(());
    };
    let trimmed = model.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_MODEL_ID_CHARS
        || trimmed.chars().any(char::is_control)
    {
        return Err(format!("model id for provider {provider_id} is invalid"));
    }
    if supports_provider_capability(capabilities, ProviderCapability::DynamicModels) {
        return Ok(());
    }
    match dcc_core::domain::model_registry::is_known_model(provider_id, trimmed) {
        Some(true) | None => Ok(()),
        Some(false) => Err(format!(
            "model {trimmed} is not in the {provider_id} catalog"
        )),
    }
}

/// Runtime-config fields that an adapter would silently ignore are rejected
/// here so a persisted preference never pretends to take effect.
pub fn validate_provider_runtime_config(
    provider_id: &str,
    capabilities: &Capabilities,
    runtime: &dcc_core::ports::ProviderRuntimeConfig,
) -> Result<(), String> {
    if runtime.binary_path.is_some()
        && !supports_provider_capability(capabilities, ProviderCapability::RuntimeBinary)
    {
        return Err(format!(
            "provider {provider_id} does not support an explicit runtime executable"
        ));
    }
    if runtime.home_path.is_some()
        && !supports_provider_capability(capabilities, ProviderCapability::RuntimeHome)
    {
        return Err(format!(
            "provider {provider_id} does not support a DCC-managed runtime home"
        ));
    }
    if runtime.shadow_home_path.is_some()
        && !supports_provider_capability(capabilities, ProviderCapability::ShadowHome)
    {
        return Err(format!(
            "provider {provider_id} does not support a DCC-managed shadow home"
        ));
    }
    if runtime.max_concurrent_subagents.is_some()
        && !supports_provider_capability(capabilities, ProviderCapability::SubagentConcurrency)
    {
        return Err(format!(
            "provider {provider_id} does not support subagent concurrency limits"
        ));
    }
    Ok(())
}

pub fn supports_provider_approval_policy(
    capabilities: &Capabilities,
    policy: ProviderApprovalPolicy,
) -> bool {
    capabilities.approval_policies.contains(&policy)
}

pub fn provider_registration(provider_id: &str) -> Option<ProviderRegistration> {
    let runtime = provider_registry().get(provider_id).cloned()?;
    let capabilities = runtime.capabilities();
    Some(ProviderRegistration {
        runtime,
        capabilities,
    })
}

/// Compatibility accessor for callers that require only the adapter object.
/// New capability-sensitive code should use [`provider_registration`].
pub fn provider_runtime(provider_id: &str) -> Option<Arc<dyn Provider>> {
    provider_registration(provider_id).map(|registration| registration.runtime)
}

async fn provider_health_statuses() -> Vec<HealthStatus> {
    let providers = provider_registry();
    let futures = PROVIDER_IDS
        .into_iter()
        .filter_map(|provider_id| providers.get(provider_id).cloned())
        .map(|provider| async move { provider.healthcheck().await });
    let results = join_all(futures).await;
    results
        .into_iter()
        .map(|result| match result {
            Ok(status) => status,
            Err(error) => HealthStatus::Unhealthy {
                reason: error.to_string(),
            },
        })
        .collect()
}

fn expose_runtime_mcp_bridge(capabilities: &mut Capabilities, provider_version: Option<&str>) {
    let Some(provider_version) = provider_version else {
        return;
    };
    if !matches!(
        capabilities.mcp_support,
        McpSupportLevel::VerifiedBridge { .. }
    ) {
        capabilities.mcp_support = McpSupportLevel::RuntimeBridge {
            provider_version: provider_version.to_string(),
        };
    }
}

fn project_catalog_capabilities(registration: &ProviderRegistration) -> Capabilities {
    let mut capabilities = registration.capabilities.clone();
    expose_runtime_mcp_bridge(
        &mut capabilities,
        registration.runtime.dcc_mcp_projection_version(),
    );
    capabilities
}

pub async fn provider_catalog() -> ProviderCatalog {
    let health = provider_health_statuses().await;
    let mut providers = Vec::with_capacity(PROVIDER_IDS.len());
    providers.push(claude_code::descriptor(health[0].clone()));
    providers.push(codex::descriptor(health[1].clone()));
    let cursor_models = cursor::discover_models().await;
    providers.push(cursor::descriptor(health[2].clone(), cursor_models));
    providers.push(grok_acp::descriptor(
        health[3].clone(),
        grok_acp::stable_grok_capabilities(),
    ));
    let antigravity_models = match provider_runtime("antigravity") {
        Some(runtime) => runtime
            .discover_models()
            .await
            .ok()
            .flatten()
            .unwrap_or_default(),
        None => Vec::new(),
    };
    providers.push(antigravity_acp::descriptor(
        health[4].clone(),
        antigravity_models,
    ));
    providers.push(gemini::descriptor(health[5].clone()));
    providers.push(droid::descriptor(health[6].clone()));
    for descriptor in &mut providers {
        // Labels/models stay adapter descriptors, while every capability comes
        // from the one canonical registered runtime. The sole catalog overlay
        // is a versioned DCC MCP bridge projection.
        if let Some(registration) = provider_registration(&descriptor.id.0) {
            descriptor.capabilities = project_catalog_capabilities(&registration);
        }
    }
    ProviderCatalog { providers }
}

pub async fn provider_descriptors() -> Vec<ProviderDescriptor> {
    provider_catalog().await.providers
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::provider::{HealthStatus, McpOauthSupport, McpSupportLevel};

    use super::{
        claude_code, codex, common::stable_cli_capabilities, droid, expose_runtime_mcp_bridge,
        gemini, grok_acp, project_catalog_capabilities, provider_registration, provider_runtime,
        supports_provider_approval_policy, supports_provider_capability, validate_provider_model,
        validate_provider_runtime_config, ProviderCapability, MAX_MODEL_ID_CHARS, PROVIDER_IDS,
    };
    use crate::cursor_mcp::CURSOR_MCP_RUNTIME_VERSION;

    #[test]
    fn registers_acp_runtimes() {
        assert!(PROVIDER_IDS.contains(&"antigravity"));
        assert!(provider_runtime("antigravity").is_some());
        assert!(PROVIDER_IDS.contains(&"grok"));
        assert!(provider_runtime("grok").is_some());
    }

    #[test]
    fn provider_order_matches_the_product_picker_order() {
        assert_eq!(
            PROVIDER_IDS,
            [
                "claude_code",
                "codex",
                "cursor",
                "grok",
                "antigravity",
                "gemini",
                "droid",
            ]
        );
    }

    #[test]
    fn registration_is_the_single_canonical_lookup_for_known_provider_ids() {
        for provider_id in PROVIDER_IDS {
            let registration = provider_registration(provider_id).expect("registered provider");
            assert_eq!(registration.runtime.id().0, provider_id);
            assert_eq!(
                serde_json::to_value(registration.runtime.capabilities()).expect("runtime caps"),
                serde_json::to_value(registration.capabilities).expect("registration caps"),
            );
        }
        assert!(provider_registration("unknown-provider").is_none());
    }

    #[test]
    fn catalog_projection_preserves_every_runtime_capability_except_versioned_mcp_overlay() {
        for provider_id in PROVIDER_IDS {
            let registration = provider_registration(provider_id).expect("registered provider");
            let mut expected = registration.capabilities.clone();
            expose_runtime_mcp_bridge(
                &mut expected,
                registration.runtime.dcc_mcp_projection_version(),
            );
            assert_eq!(
                serde_json::to_value(project_catalog_capabilities(&registration))
                    .expect("catalog caps"),
                serde_json::to_value(expected).expect("expected caps"),
            );
        }
    }

    #[test]
    fn pure_capability_checks_cover_approval_and_existing_control_paths() {
        let codex = provider_registration("codex").expect("Codex provider");
        assert!(supports_provider_capability(
            &codex.capabilities,
            ProviderCapability::Steering
        ));
        assert!(supports_provider_capability(
            &codex.capabilities,
            ProviderCapability::MultiRoot
        ));
        assert!(supports_provider_approval_policy(
            &codex.capabilities,
            dcc_core::domain::provider::ProviderApprovalPolicy::Ask
        ));

        let antigravity = provider_registration("antigravity").expect("Antigravity provider");
        assert!(!supports_provider_capability(
            &antigravity.capabilities,
            ProviderCapability::MultiRoot
        ));
        assert!(supports_provider_approval_policy(
            &antigravity.capabilities,
            dcc_core::domain::provider::ProviderApprovalPolicy::Ask
        ));
        assert!(supports_provider_approval_policy(
            &antigravity.capabilities,
            dcc_core::domain::provider::ProviderApprovalPolicy::Auto
        ));
        assert!(supports_provider_approval_policy(
            &antigravity.capabilities,
            dcc_core::domain::provider::ProviderApprovalPolicy::FullAccess
        ));

        let droid = provider_registration("droid").expect("Droid provider");
        assert!(!supports_provider_capability(
            &droid.capabilities,
            ProviderCapability::Steering
        ));
        assert!(!supports_provider_capability(
            &droid.capabilities,
            ProviderCapability::MultiRoot
        ));
        assert!(!supports_provider_approval_policy(
            &droid.capabilities,
            dcc_core::domain::provider::ProviderApprovalPolicy::Ask
        ));
    }

    #[test]
    fn only_adapters_with_an_explicit_versioned_projection_channel_accept_dcc_mcp() {
        assert_eq!(
            provider_runtime("claude_code")
                .expect("Claude provider")
                .dcc_mcp_projection_version(),
            Some("claude-agent-sdk@0.2.126+claude-code@2.1.258")
        );
        assert!(matches!(
            provider_runtime("codex")
                .expect("Codex provider")
                .dcc_mcp_projection_version(),
            None | Some(_)
        ));
        assert!(matches!(
            provider_runtime("cursor")
                .expect("Cursor provider")
                .dcc_mcp_projection_version(),
            None | Some(CURSOR_MCP_RUNTIME_VERSION)
        ));
        for provider_id in PROVIDER_IDS
            .into_iter()
            .filter(|provider_id| !matches!(*provider_id, "claude_code" | "codex" | "cursor"))
        {
            assert_eq!(
                provider_runtime(provider_id)
                    .expect("registered provider")
                    .dcc_mcp_projection_version(),
                None
            );
        }
    }

    #[test]
    fn runtime_usage_and_concurrency_capabilities_match_each_adapter_contract() {
        let expectations: [(&str, bool, bool, bool, bool, bool); 7] = [
            // provider, runtime home, binary path, shadow home, subagent concurrency, usage
            ("claude_code", true, false, false, false, true),
            ("codex", true, false, true, true, true),
            ("antigravity", true, true, false, false, false),
            ("gemini", true, true, false, false, false),
            ("droid", false, false, false, false, false),
            ("cursor", false, false, false, false, false),
            ("grok", true, false, false, false, false),
        ];
        for (provider_id, runtime_home, runtime_binary, shadow_home, concurrency, usage) in
            expectations
        {
            let registration = provider_registration(provider_id).expect("registered provider");
            let capabilities = &registration.capabilities;
            assert_eq!(
                supports_provider_capability(capabilities, ProviderCapability::RuntimeHome),
                runtime_home,
                "{provider_id} runtime home"
            );
            assert_eq!(
                supports_provider_capability(capabilities, ProviderCapability::RuntimeBinary),
                runtime_binary,
                "{provider_id} runtime binary"
            );
            assert_eq!(
                supports_provider_capability(capabilities, ProviderCapability::ShadowHome),
                shadow_home,
                "{provider_id} shadow home"
            );
            assert_eq!(
                supports_provider_capability(capabilities, ProviderCapability::SubagentConcurrency),
                concurrency,
                "{provider_id} subagent concurrency"
            );
            assert_eq!(
                supports_provider_capability(capabilities, ProviderCapability::AccountUsage),
                usage,
                "{provider_id} account usage"
            );
        }
    }

    #[test]
    fn runtime_config_validation_rejects_fields_an_adapter_would_ignore() {
        use dcc_core::ports::ProviderRuntimeConfig;

        let droid = provider_registration("droid").expect("Droid provider");
        assert!(validate_provider_runtime_config(
            "droid",
            &droid.capabilities,
            &ProviderRuntimeConfig::default()
        )
        .is_ok());
        assert!(validate_provider_runtime_config(
            "droid",
            &droid.capabilities,
            &ProviderRuntimeConfig {
                binary_path: None,
                home_path: Some("/tmp/home".to_string()),
                ..ProviderRuntimeConfig::default()
            }
        )
        .is_err());
        assert!(validate_provider_runtime_config(
            "droid",
            &droid.capabilities,
            &ProviderRuntimeConfig {
                max_concurrent_subagents: Some(2),
                ..ProviderRuntimeConfig::default()
            }
        )
        .is_err());

        let claude = provider_registration("claude_code").expect("Claude provider");
        assert!(validate_provider_runtime_config(
            "claude_code",
            &claude.capabilities,
            &ProviderRuntimeConfig {
                home_path: Some("/tmp/home".to_string()),
                ..ProviderRuntimeConfig::default()
            }
        )
        .is_ok());
        assert!(validate_provider_runtime_config(
            "claude_code",
            &claude.capabilities,
            &ProviderRuntimeConfig {
                shadow_home_path: Some("/tmp/shadow".to_string()),
                ..ProviderRuntimeConfig::default()
            }
        )
        .is_err());

        let codex = provider_registration("codex").expect("Codex provider");
        assert!(validate_provider_runtime_config(
            "codex",
            &codex.capabilities,
            &ProviderRuntimeConfig {
                binary_path: None,
                home_path: Some("/tmp/home".to_string()),
                shadow_home_path: Some("/tmp/shadow".to_string()),
                max_concurrent_subagents: Some(4),
            }
        )
        .is_ok());
    }

    #[test]
    fn model_authority_follows_the_static_catalog_unless_models_are_dynamic() {
        let codex = provider_registration("codex").expect("Codex provider");
        assert!(validate_provider_model("codex", &codex.capabilities, None).is_ok());
        let known = dcc_core::domain::model_registry::CODEX[0].id;
        assert!(validate_provider_model("codex", &codex.capabilities, Some(known)).is_ok());
        assert!(
            validate_provider_model("codex", &codex.capabilities, Some("made-up-model")).is_err()
        );
        assert!(validate_provider_model("codex", &codex.capabilities, Some("  ")).is_err());
        assert!(validate_provider_model(
            "codex",
            &codex.capabilities,
            Some(&"m".repeat(MAX_MODEL_ID_CHARS + 1))
        )
        .is_err());
        let (alias, canonical) = dcc_core::domain::model_registry::CODEX_ALIASES[0];
        assert!(validate_provider_model("codex", &codex.capabilities, Some(alias)).is_ok());
        assert!(validate_provider_model("codex", &codex.capabilities, Some(canonical)).is_ok());

        let cursor = provider_registration("cursor").expect("Cursor provider");
        assert!(supports_provider_capability(
            &cursor.capabilities,
            ProviderCapability::DynamicModels
        ));
        assert!(validate_provider_model(
            "cursor",
            &cursor.capabilities,
            Some("runtime-discovered")
        )
        .is_ok());
        assert!(
            validate_provider_model("cursor", &cursor.capabilities, Some("bad\u{0007}id")).is_err()
        );
    }

    #[test]
    fn plan_fast_and_compaction_support_match_each_adapter_contract() {
        use dcc_core::domain::provider::TurnControlSupport::{Native, PromptFallback};
        let expectations = [
            ("claude_code", Native, Native, true),
            ("codex", Native, Native, false),
            ("antigravity", PromptFallback, PromptFallback, false),
            ("gemini", Native, PromptFallback, false),
            ("droid", Native, PromptFallback, false),
            ("cursor", Native, PromptFallback, false),
            ("grok", PromptFallback, PromptFallback, false),
        ];
        for (provider_id, plan, fast, compaction) in expectations {
            let registration = provider_registration(provider_id).expect("registered provider");
            assert_eq!(
                registration.capabilities.plan_mode_support, plan,
                "{provider_id} plan"
            );
            assert_eq!(
                registration.capabilities.fast_mode_support, fast,
                "{provider_id} fast"
            );
            assert_eq!(
                supports_provider_capability(
                    &registration.capabilities,
                    ProviderCapability::CompactionCommand
                ),
                compaction,
                "{provider_id} compaction"
            );
        }
    }

    #[test]
    fn stable_preset_does_not_claim_mcp_attachment() {
        assert_eq!(
            stable_cli_capabilities().mcp_support,
            McpSupportLevel::Unsupported
        );
    }

    #[test]
    fn provider_mcp_levels_match_the_current_adapter_contracts() {
        let healthy = HealthStatus::Healthy;

        assert_eq!(
            claude_code::descriptor(healthy.clone())
                .capabilities
                .mcp_support,
            McpSupportLevel::NativeConfig
        );
        assert_eq!(
            codex::descriptor(healthy.clone()).capabilities.mcp_support,
            McpSupportLevel::NativeConfig
        );
        assert_eq!(
            gemini::descriptor(healthy.clone()).capabilities.mcp_support,
            McpSupportLevel::Unsupported
        );
        assert_eq!(
            droid::descriptor(healthy.clone()).capabilities.mcp_support,
            McpSupportLevel::Unsupported
        );
        assert_eq!(
            crate::cursor::descriptor(healthy.clone(), Vec::new())
                .capabilities
                .mcp_support,
            McpSupportLevel::NativeConfig
        );
        assert_eq!(
            grok_acp::descriptor(healthy, grok_acp::stable_grok_capabilities())
                .capabilities
                .mcp_support,
            McpSupportLevel::Unsupported
        );
    }

    #[test]
    fn provider_oauth_flow_matches_each_adapter_contract() {
        assert_eq!(
            claude_code::descriptor(HealthStatus::Healthy)
                .capabilities
                .mcp_oauth_support,
            McpOauthSupport::ManagedDuringTurn
        );
        assert_eq!(
            codex::descriptor(HealthStatus::Healthy)
                .capabilities
                .mcp_oauth_support,
            McpOauthSupport::InteractivePreflight
        );
        assert_eq!(
            stable_cli_capabilities().mcp_oauth_support,
            McpOauthSupport::Unsupported
        );
    }

    #[test]
    fn runtime_projection_is_visible_without_claiming_verified_conformance() {
        let mut capabilities = codex::descriptor(HealthStatus::Healthy).capabilities;
        expose_runtime_mcp_bridge(
            &mut capabilities,
            Some("codex-cli@0.146.0+app-server-protocol-v2"),
        );
        assert_eq!(
            capabilities.mcp_support,
            McpSupportLevel::RuntimeBridge {
                provider_version: "codex-cli@0.146.0+app-server-protocol-v2".to_string(),
            }
        );
        assert!(capabilities.mcp_support.verified_evidence().is_none());
    }
}
