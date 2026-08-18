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
    HealthStatus, McpSupportLevel, ProviderCatalog, ProviderDescriptor,
};
use dcc_core::ports::Provider;

pub const PROVIDER_IDS: [&str; 6] = ["claude_code", "codex", "gemini", "droid", "cursor", "grok"];

fn provider_registry() -> &'static HashMap<String, Arc<dyn Provider>> {
    static REGISTRY: OnceLock<HashMap<String, Arc<dyn Provider>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let providers: [Arc<dyn Provider>; 6] = [
            Arc::new(claude_code::adapter()),
            Arc::new(codex::adapter()),
            Arc::new(gemini::adapter()),
            Arc::new(droid::adapter()),
            Arc::new(cursor::adapter()),
            Arc::new(grok_acp::GrokAcpAdapter::new(
                common::stable_cli_capabilities(),
            )),
        ];

        let mut registry = HashMap::with_capacity(PROVIDER_IDS.len());
        for provider in providers {
            registry.insert(provider.id().0.clone(), provider);
        }
        registry
    })
}

pub fn provider_runtime(provider_id: &str) -> Option<Arc<dyn Provider>> {
    provider_registry().get(provider_id).cloned()
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

fn expose_runtime_mcp_bridge(descriptor: &mut ProviderDescriptor, provider_version: Option<&str>) {
    let Some(provider_version) = provider_version else {
        return;
    };
    if !matches!(
        descriptor.capabilities.mcp_support,
        McpSupportLevel::VerifiedBridge { .. }
    ) {
        descriptor.capabilities.mcp_support = McpSupportLevel::RuntimeBridge {
            provider_version: provider_version.to_string(),
        };
    }
}

pub async fn provider_catalog() -> ProviderCatalog {
    let health = provider_health_statuses().await;
    let mut providers = Vec::with_capacity(PROVIDER_IDS.len());
    providers.push(claude_code::descriptor(health[0].clone()));
    providers.push(codex::descriptor(health[1].clone()));
    providers.push(gemini::descriptor(health[2].clone()));
    providers.push(droid::descriptor(health[3].clone()));
    let cursor_models = cursor::discover_models().await;
    providers.push(cursor::descriptor(health[4].clone(), cursor_models));
    providers.push(grok_acp::descriptor(
        health[5].clone(),
        common::stable_cli_capabilities(),
    ));
    for descriptor in &mut providers {
        let runtime = provider_registry().get(&descriptor.id.0);
        let runtime_version = runtime.and_then(|provider| provider.dcc_mcp_projection_version());
        expose_runtime_mcp_bridge(descriptor, runtime_version);
        if let Some(runtime) = runtime {
            let capabilities = runtime.capabilities();
            descriptor.capabilities.supports_native_subagent_steering =
                capabilities.supports_native_subagent_steering;
            descriptor.capabilities.supports_native_subagent_interrupt =
                capabilities.supports_native_subagent_interrupt;
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
        gemini, grok_acp, provider_runtime, PROVIDER_IDS,
    };
    use crate::cursor_mcp::CURSOR_MCP_RUNTIME_VERSION;

    #[test]
    fn registers_grok_runtime() {
        assert!(PROVIDER_IDS.contains(&"grok"));
        assert!(provider_runtime("grok").is_some());
    }

    #[test]
    fn only_adapters_with_an_explicit_versioned_projection_channel_accept_dcc_mcp() {
        assert_eq!(
            provider_runtime("claude_code")
                .expect("Claude provider")
                .dcc_mcp_projection_version(),
            Some("claude-agent-sdk@0.2.126+claude-code@2.1.126")
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
            grok_acp::descriptor(healthy, stable_cli_capabilities())
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
        let mut descriptor = codex::descriptor(HealthStatus::Healthy);
        expose_runtime_mcp_bridge(
            &mut descriptor,
            Some("codex-cli@0.146.0+app-server-protocol-v2"),
        );
        assert_eq!(
            descriptor.capabilities.mcp_support,
            McpSupportLevel::RuntimeBridge {
                provider_version: "codex-cli@0.146.0+app-server-protocol-v2".to_string(),
            }
        );
        assert!(descriptor
            .capabilities
            .mcp_support
            .verified_evidence()
            .is_none());
    }
}
