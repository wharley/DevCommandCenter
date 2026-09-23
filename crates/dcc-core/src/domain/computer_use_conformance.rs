use chrono::DateTime;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::{mcp::McpTransportKind, provider::ProviderId};

pub const COMPUTER_USE_CONFORMANCE_SUITE_VERSION: &str = "dcc-computer-use-provider-conformance-v1";
pub const COMPUTER_USE_CONFORMANCE_FIXTURE_VERSION: &str = "dcc-computer-use-fixture-v1";

const REQUIRED_CHECKS: [ComputerUseConformanceCheck; 2] = [
    ComputerUseConformanceCheck::McpImageUnderstood,
    ComputerUseConformanceCheck::ActiveTurnInterrupted,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ComputerUseConformanceCheck {
    McpImageUnderstood,
    ActiveTurnInterrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseConformanceTransportEvidence {
    transport: McpTransportKind,
    checks: Vec<ComputerUseConformanceCheck>,
}

impl ComputerUseConformanceTransportEvidence {
    pub fn transport(&self) -> &McpTransportKind {
        &self.transport
    }

    pub fn checks(&self) -> &[ComputerUseConformanceCheck] {
        &self.checks
    }
}

/// Evidence produced by the shared Computer Use provider gate. This remains
/// separate from `McpConformanceEvidence`: tool bridging does not prove visual
/// input or interruption behavior.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseConformanceEvidence {
    provider_id: ProviderId,
    provider_version: String,
    suite_version: String,
    fixture_version: String,
    transports: Vec<ComputerUseConformanceTransportEvidence>,
    verified_at: String,
}

impl ComputerUseConformanceEvidence {
    pub(crate) fn from_successful_run(
        provider_id: ProviderId,
        provider_version: String,
        verified_at: String,
    ) -> Result<Self, ComputerUseConformanceEvidenceError> {
        let evidence = Self {
            provider_id,
            provider_version,
            suite_version: COMPUTER_USE_CONFORMANCE_SUITE_VERSION.to_string(),
            fixture_version: COMPUTER_USE_CONFORMANCE_FIXTURE_VERSION.to_string(),
            transports: [McpTransportKind::Stdio, McpTransportKind::Http]
                .into_iter()
                .map(|transport| ComputerUseConformanceTransportEvidence {
                    transport,
                    checks: REQUIRED_CHECKS.to_vec(),
                })
                .collect(),
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

    pub fn transports(&self) -> &[ComputerUseConformanceTransportEvidence] {
        &self.transports
    }

    pub fn verified_at(&self) -> &str {
        &self.verified_at
    }

    pub fn validate(&self) -> Result<(), ComputerUseConformanceEvidenceError> {
        if !valid_metadata(&self.provider_id.0) || !valid_metadata(&self.provider_version) {
            return Err(ComputerUseConformanceEvidenceError::InvalidProviderMetadata);
        }
        if self.suite_version != COMPUTER_USE_CONFORMANCE_SUITE_VERSION {
            return Err(ComputerUseConformanceEvidenceError::UnsupportedSuiteVersion);
        }
        if self.fixture_version != COMPUTER_USE_CONFORMANCE_FIXTURE_VERSION {
            return Err(ComputerUseConformanceEvidenceError::UnsupportedFixtureVersion);
        }
        if DateTime::parse_from_rfc3339(&self.verified_at).is_err() {
            return Err(ComputerUseConformanceEvidenceError::InvalidVerifiedAt);
        }
        let expected = [McpTransportKind::Stdio, McpTransportKind::Http];
        if self.transports.len() != expected.len()
            || self
                .transports
                .iter()
                .zip(expected)
                .any(|(actual, expected)| actual.transport != expected)
        {
            return Err(ComputerUseConformanceEvidenceError::IncompleteTransportCoverage);
        }
        if self
            .transports
            .iter()
            .any(|transport| transport.checks != REQUIRED_CHECKS)
        {
            return Err(ComputerUseConformanceEvidenceError::IncompleteCheckCoverage);
        }
        Ok(())
    }

    pub fn validate_for_provider(
        &self,
        provider_id: &ProviderId,
        provider_version: &str,
    ) -> Result<(), ComputerUseConformanceEvidenceError> {
        self.validate()?;
        if &self.provider_id != provider_id || self.provider_version != provider_version {
            return Err(ComputerUseConformanceEvidenceError::ProviderVersionMismatch);
        }
        Ok(())
    }
}

fn valid_metadata(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ComputerUseConformanceEvidenceError {
    #[error("Computer Use conformance metadata is invalid")]
    InvalidProviderMetadata,
    #[error("Computer Use conformance suite version is unsupported")]
    UnsupportedSuiteVersion,
    #[error("Computer Use conformance fixture version is unsupported")]
    UnsupportedFixtureVersion,
    #[error("Computer Use conformance timestamp is invalid")]
    InvalidVerifiedAt,
    #[error("Computer Use conformance transport coverage is incomplete")]
    IncompleteTransportCoverage,
    #[error("Computer Use conformance check coverage is incomplete")]
    IncompleteCheckCoverage,
    #[error("Computer Use conformance evidence does not match the active provider version")]
    ProviderVersionMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence() -> ComputerUseConformanceEvidence {
        ComputerUseConformanceEvidence::from_successful_run(
            ProviderId("codex".to_string()),
            "codex-cli@fixture-version".to_string(),
            "2026-09-23T12:00:00Z".to_string(),
        )
        .expect("valid Computer Use evidence")
    }

    #[test]
    fn successful_evidence_records_image_and_interruption_on_both_transports() {
        let evidence = evidence();
        evidence.validate().expect("complete evidence");
        assert_eq!(evidence.transports().len(), 2);
        assert!(evidence
            .transports()
            .iter()
            .all(|transport| transport.checks() == REQUIRED_CHECKS));
        let serialized = serde_json::to_string(&evidence).expect("serialize evidence");
        assert!(serialized.contains(COMPUTER_USE_CONFORMANCE_SUITE_VERSION));
        assert!(!serialized.contains("COPPER-HARBOR-61"));
    }

    #[test]
    fn incomplete_evidence_and_runtime_version_mismatch_are_rejected() {
        let evidence = evidence();
        let mut value = serde_json::to_value(&evidence).expect("serialize evidence");
        value["transports"][0]["checks"]
            .as_array_mut()
            .expect("checks")
            .pop();
        let incomplete: ComputerUseConformanceEvidence =
            serde_json::from_value(value).expect("deserialize evidence");
        assert_eq!(
            incomplete.validate(),
            Err(ComputerUseConformanceEvidenceError::IncompleteCheckCoverage)
        );
        assert_eq!(
            evidence.validate_for_provider(&ProviderId("codex".to_string()), "newer-version"),
            Err(ComputerUseConformanceEvidenceError::ProviderVersionMismatch)
        );
    }
}
