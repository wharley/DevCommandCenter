use chrono::DateTime;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::{mcp::McpTransportKind, provider::ProviderId};

pub const MCP_CONFORMANCE_SUITE_VERSION: &str = "dcc-mcp-provider-conformance-v1";
pub const MCP_CONFORMANCE_FIXTURE_VERSION: &str = "dcc-mcp-fixture-v1";
pub const MCP_CONFORMANCE_ECHO_TOOL: &str = "fixture.echo";
pub const MCP_CONFORMANCE_MUTATING_TOOL: &str = "fixture.mutate";

const MAX_PROVIDER_METADATA_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum McpConformanceCheck {
    FixtureAttached,
    SessionCreated,
    ToolsVisible,
    ReadOnlyCall,
    MutatingApproval,
    Disabled,
    Removed,
    ServerUnavailableFailsClosed,
    CredentialUnavailableFailsClosed,
}

pub const REQUIRED_MCP_CONFORMANCE_CHECKS: [McpConformanceCheck; 9] = [
    McpConformanceCheck::FixtureAttached,
    McpConformanceCheck::SessionCreated,
    McpConformanceCheck::ToolsVisible,
    McpConformanceCheck::ReadOnlyCall,
    McpConformanceCheck::MutatingApproval,
    McpConformanceCheck::Disabled,
    McpConformanceCheck::Removed,
    McpConformanceCheck::ServerUnavailableFailsClosed,
    McpConformanceCheck::CredentialUnavailableFailsClosed,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConformanceTransportEvidence {
    transport: McpTransportKind,
    checks: Vec<McpConformanceCheck>,
}

impl McpConformanceTransportEvidence {
    pub fn transport(&self) -> &McpTransportKind {
        &self.transport
    }

    pub fn checks(&self) -> &[McpConformanceCheck] {
        &self.checks
    }
}

/// Versioned, secret-free evidence produced only after the shared provider
/// conformance harness completes successfully.
///
/// Fields are deliberately private. Provider adapters can consume persisted
/// evidence, but normal Rust code cannot mint a `verifiedBridge` attestation by
/// assembling a struct and bypassing the harness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct McpConformanceEvidence {
    provider_id: ProviderId,
    provider_version: String,
    suite_version: String,
    fixture_version: String,
    transports: Vec<McpConformanceTransportEvidence>,
    verified_at: String,
}

impl McpConformanceEvidence {
    pub(crate) fn from_successful_run(
        provider_id: ProviderId,
        provider_version: String,
        verified_at: String,
    ) -> Result<Self, McpConformanceEvidenceError> {
        let evidence = Self {
            provider_id,
            provider_version,
            suite_version: MCP_CONFORMANCE_SUITE_VERSION.to_string(),
            fixture_version: MCP_CONFORMANCE_FIXTURE_VERSION.to_string(),
            transports: vec![
                McpConformanceTransportEvidence {
                    transport: McpTransportKind::Stdio,
                    checks: REQUIRED_MCP_CONFORMANCE_CHECKS.to_vec(),
                },
                McpConformanceTransportEvidence {
                    transport: McpTransportKind::Http,
                    checks: REQUIRED_MCP_CONFORMANCE_CHECKS.to_vec(),
                },
            ],
            verified_at,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn provider_version(&self) -> &str {
        &self.provider_version
    }

    pub fn suite_version(&self) -> &str {
        &self.suite_version
    }

    pub fn fixture_version(&self) -> &str {
        &self.fixture_version
    }

    pub fn transports(&self) -> &[McpConformanceTransportEvidence] {
        &self.transports
    }

    pub fn verified_at(&self) -> &str {
        &self.verified_at
    }

    /// Revalidates evidence loaded from persistence before it is used to
    /// advertise provider compatibility.
    pub fn validate(&self) -> Result<(), McpConformanceEvidenceError> {
        validate_provider_metadata(&self.provider_id.0)?;
        validate_provider_metadata(&self.provider_version)?;
        if self.suite_version != MCP_CONFORMANCE_SUITE_VERSION {
            return Err(McpConformanceEvidenceError::UnsupportedSuiteVersion);
        }
        if self.fixture_version != MCP_CONFORMANCE_FIXTURE_VERSION {
            return Err(McpConformanceEvidenceError::UnsupportedFixtureVersion);
        }
        if DateTime::parse_from_rfc3339(&self.verified_at).is_err() {
            return Err(McpConformanceEvidenceError::InvalidVerifiedAt);
        }

        let expected_transports = [McpTransportKind::Stdio, McpTransportKind::Http];
        if self.transports.len() != expected_transports.len() {
            return Err(McpConformanceEvidenceError::IncompleteTransportCoverage);
        }
        for (evidence, expected_transport) in self.transports.iter().zip(expected_transports.iter())
        {
            if evidence.transport != *expected_transport {
                return Err(McpConformanceEvidenceError::IncompleteTransportCoverage);
            }
            if evidence.checks != REQUIRED_MCP_CONFORMANCE_CHECKS {
                return Err(McpConformanceEvidenceError::IncompleteCheckCoverage);
            }
        }

        Ok(())
    }

    /// Validates both the evidence shape and the exact provider runtime it is
    /// being used to advertise. Provider upgrades therefore fail closed until
    /// that version completes the suite.
    pub fn validate_for_provider(
        &self,
        provider_id: &ProviderId,
        provider_version: &str,
    ) -> Result<(), McpConformanceEvidenceError> {
        self.validate()?;
        if &self.provider_id != provider_id || self.provider_version != provider_version {
            return Err(McpConformanceEvidenceError::ProviderVersionMismatch);
        }
        Ok(())
    }
}

fn validate_provider_metadata(value: &str) -> Result<(), McpConformanceEvidenceError> {
    if value.trim().is_empty()
        || value.chars().count() > MAX_PROVIDER_METADATA_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(McpConformanceEvidenceError::InvalidProviderMetadata);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum McpConformanceEvidenceError {
    #[error("provider conformance metadata is invalid")]
    InvalidProviderMetadata,
    #[error("provider conformance suite version is unsupported")]
    UnsupportedSuiteVersion,
    #[error("provider conformance fixture version is unsupported")]
    UnsupportedFixtureVersion,
    #[error("provider conformance timestamp is invalid")]
    InvalidVerifiedAt,
    #[error("provider conformance transport coverage is incomplete")]
    IncompleteTransportCoverage,
    #[error("provider conformance check coverage is incomplete")]
    IncompleteCheckCoverage,
    #[error("provider conformance evidence does not match the active provider version")]
    ProviderVersionMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_evidence_contains_only_fixed_metadata_and_complete_coverage() {
        let evidence = McpConformanceEvidence::from_successful_run(
            ProviderId("fixture-provider".to_string()),
            "1.2.3".to_string(),
            "2026-07-28T12:00:00Z".to_string(),
        )
        .expect("valid evidence");

        evidence.validate().expect("complete evidence");
        let json = serde_json::to_string(&evidence).expect("serialize evidence");
        assert!(json.contains(MCP_CONFORMANCE_SUITE_VERSION));
        assert!(json.contains("fixture-provider"));
        assert!(!json.contains("echo payload"));
        assert!(!json.contains("super-secret-value"));
    }

    #[test]
    fn deserialized_evidence_cannot_omit_a_required_check() {
        let evidence = McpConformanceEvidence::from_successful_run(
            ProviderId("fixture-provider".to_string()),
            "1.2.3".to_string(),
            "2026-07-28T12:00:00Z".to_string(),
        )
        .expect("valid evidence");
        let mut value = serde_json::to_value(evidence).expect("serialize evidence");
        value["transports"][0]["checks"]
            .as_array_mut()
            .expect("checks")
            .pop();
        let incomplete: McpConformanceEvidence =
            serde_json::from_value(value).expect("deserialize evidence");

        assert_eq!(
            incomplete.validate(),
            Err(McpConformanceEvidenceError::IncompleteCheckCoverage)
        );
    }

    #[test]
    fn evidence_is_bound_to_the_exact_provider_version() {
        let evidence = McpConformanceEvidence::from_successful_run(
            ProviderId("fixture-provider".to_string()),
            "1.2.3".to_string(),
            "2026-07-28T12:00:00Z".to_string(),
        )
        .expect("valid evidence");

        assert_eq!(
            evidence.validate_for_provider(&ProviderId("fixture-provider".to_string()), "1.2.4"),
            Err(McpConformanceEvidenceError::ProviderVersionMismatch)
        );
    }
}
