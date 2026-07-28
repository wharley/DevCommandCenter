use crate::{
    domain::mcp::{McpDefinition, McpErrorCategory, McpProbeReport, McpRuntimeError},
    ports::McpProbe,
};

pub async fn probe_mcp_definition<P>(
    probe: &P,
    definition: &McpDefinition,
) -> std::result::Result<McpProbeReport, McpRuntimeError>
where
    P: McpProbe + Sync + ?Sized,
{
    definition.validate().map_err(|_| {
        McpRuntimeError::bounded(
            McpErrorCategory::InvalidDefinition,
            "MCP definition is invalid",
        )
    })?;
    if definition.trust.requires_confirmation() {
        return Err(McpRuntimeError::bounded(
            McpErrorCategory::PermissionBoundary,
            "MCP definition requires explicit trust before probing",
        ));
    }

    let report = probe.probe(definition).await?;
    report.validate().map_err(|_| {
        McpRuntimeError::bounded(
            McpErrorCategory::Protocol,
            "MCP probe returned an invalid report",
        )
    })?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        domain::mcp::{
            McpDefinitionId, McpDefinitionOwnership, McpTransport, McpTrust, McpTrustDecision,
            McpTrustFingerprint,
        },
        ports::McpProbeResult,
    };

    struct FakeProbe;

    #[async_trait]
    impl McpProbe for FakeProbe {
        async fn probe(&self, definition: &McpDefinition) -> McpProbeResult<McpProbeReport> {
            Ok(McpProbeReport {
                definition_id: definition.id.clone(),
                transport: definition.transport.kind(),
                protocol_version: "2025-11-25".to_string(),
                tools: Vec::new(),
                checked_at: "2026-07-28T00:00:00Z".to_string(),
            })
        }
    }

    fn definition() -> McpDefinition {
        let mut definition = McpDefinition {
            id: McpDefinitionId("fixture".to_string()),
            display_name: "Fixture".to_string(),
            transport: McpTransport::Http {
                url: "http://127.0.0.1:8765/mcp".to_string(),
            },
            secret_refs: Vec::new(),
            enabled: false,
            ownership: McpDefinitionOwnership::DccManaged,
            trust: McpTrust {
                current_fingerprint: McpTrustFingerprint("0".repeat(64)),
                decision: McpTrustDecision::Untrusted,
            },
            created_at: "2026-07-28T00:00:00Z".to_string(),
            updated_at: "2026-07-28T00:00:00Z".to_string(),
        };
        definition.synchronize_trust_fingerprint();
        definition
    }

    #[test]
    fn probe_requires_trust_even_when_it_does_not_enable_the_definition() {
        let error = futures::executor::block_on(probe_mcp_definition(&FakeProbe, &definition()))
            .expect_err("untrusted probe must fail");

        assert_eq!(error.category, McpErrorCategory::PermissionBoundary);
    }

    #[test]
    fn trusted_disabled_definition_can_be_probed_explicitly() {
        let mut definition = definition();
        let fingerprint = definition.trust.current_fingerprint.clone();
        definition.trust.decision = McpTrustDecision::Trusted {
            fingerprint,
            trusted_at: "2026-07-28T00:00:00Z".to_string(),
        };

        let report = futures::executor::block_on(probe_mcp_definition(&FakeProbe, &definition))
            .expect("trusted probe");
        assert_eq!(report.definition_id, definition.id);
        assert!(!definition.enabled);
    }
}
