//! A provider's chips in the Session tab's catalog (`provider-display.ts`):
//! stable or experimental, its health, what it can do, and its models.

use dcc_core::domain::provider::{HealthStatus, McpSupportLevel, ProviderDescriptor};

/// The chip's look (`ProviderChip.variant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    Success,
    Warn,
    Destructive,
    Outline,
}

/// `summarizeProviderHealth`
pub fn health(health: &HealthStatus) -> (String, Chip) {
    match health {
        HealthStatus::Healthy => ("healthy".into(), Chip::Success),
        HealthStatus::Degraded { reason } => (reason.clone(), Chip::Warn),
        HealthStatus::Unhealthy { reason } => (reason.clone(), Chip::Destructive),
    }
}

/// `getProviderChips` without the first two (stability and health), as
/// `ProviderCatalogDense` shows them.
pub fn capability_chips(provider: &ProviderDescriptor) -> Vec<(&'static str, Chip)> {
    let capabilities = &provider.capabilities;
    let mcp = match capabilities.mcp_support {
        McpSupportLevel::VerifiedBridge { .. } => Some(("mcp verified", Chip::Success)),
        McpSupportLevel::RuntimeBridge { .. } => Some(("mcp runtime bridge", Chip::Warn)),
        McpSupportLevel::NativeConfig => Some(("mcp native config", Chip::Outline)),
        McpSupportLevel::Unsupported => None,
    };
    let (health_label, _) = health(&provider.health);
    [
        capabilities
            .streaming
            .then_some(("streaming", Chip::Outline)),
        capabilities.tools.then_some(("tools", Chip::Outline)),
        mcp,
        capabilities
            .resumable
            .then_some(("resumable", Chip::Outline)),
        capabilities.vision.then_some(("vision", Chip::Outline)),
    ]
    .into_iter()
    .flatten()
    // The desktop drops a chip that repeats the stability or the health.
    .filter(|(label, _)| *label != "stable" && *label != "experimental" && *label != health_label)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_read_like_the_desktop() {
        let mut provider = dcc_providers::offline_catalog().providers.remove(0);
        provider.health = HealthStatus::Degraded {
            reason: "slow".into(),
        };
        provider.capabilities.streaming = true;
        provider.capabilities.vision = false;
        provider.capabilities.mcp_support = McpSupportLevel::NativeConfig;
        assert_eq!(health(&provider.health), ("slow".into(), Chip::Warn));
        let chips = capability_chips(&provider);
        assert_eq!(chips.first(), Some(&("streaming", Chip::Outline)));
        assert!(chips.contains(&("mcp native config", Chip::Outline)));
        assert!(!chips.iter().any(|(label, _)| *label == "vision"));
    }
}
