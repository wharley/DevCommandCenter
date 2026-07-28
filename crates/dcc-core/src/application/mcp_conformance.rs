use chrono::Utc;
use thiserror::Error;

use crate::{
    domain::{
        mcp::McpTransportKind,
        mcp_conformance::{
            McpConformanceCheck, McpConformanceEvidence, McpConformanceEvidenceError,
            MCP_CONFORMANCE_ECHO_TOOL, MCP_CONFORMANCE_MUTATING_TOOL,
            REQUIRED_MCP_CONFORMANCE_CHECKS,
        },
    },
    ports::mcp_conformance::{
        McpConformanceAdapter, McpConformanceAdapterError, McpConformanceObservation,
        McpConformanceStep, McpConformanceUnavailableKind, MCP_CONFORMANCE_ECHO_VALUE,
    },
};

const MAX_VISIBLE_TOOLS: usize = 256;
const MAX_TOOL_NAME_CHARS: usize = 128;

/// Runs the complete provider-neutral MCP bridge gate for stdio and HTTP.
///
/// Evidence is returned only after both transports pass every behavior. A
/// categorical error is returned otherwise, without forwarding provider
/// output, tool payloads, or credentials.
pub async fn run_provider_mcp_conformance<A>(
    adapter: &mut A,
) -> Result<McpConformanceEvidence, McpConformanceFailure>
where
    A: McpConformanceAdapter + ?Sized,
{
    let provider_id = adapter.provider_id();
    let provider_version = adapter.provider_version();
    validate_metadata(&provider_id.0, &provider_version)?;

    for transport in [McpTransportKind::Stdio, McpTransportKind::Http] {
        run_transport(adapter, transport).await?;
    }

    McpConformanceEvidence::from_successful_run(
        provider_id,
        provider_version,
        Utc::now().to_rfc3339(),
    )
    .map_err(McpConformanceFailure::EvidenceRejected)
}

async fn run_transport<A>(
    adapter: &mut A,
    transport: McpTransportKind,
) -> Result<(), McpConformanceFailure>
where
    A: McpConformanceAdapter + ?Sized,
{
    let result = run_transport_scenarios(adapter, transport.clone()).await;
    let cleanup = expect_exact(
        adapter,
        &transport,
        McpConformanceStep::FinalCleanup,
        McpConformanceObservation::CleanupConfirmed,
    )
    .await;

    match result {
        Err(error) => Err(error),
        Ok(()) => cleanup,
    }
}

async fn run_transport_scenarios<A>(
    adapter: &mut A,
    transport: McpTransportKind,
) -> Result<(), McpConformanceFailure>
where
    A: McpConformanceAdapter + ?Sized,
{
    let mut completed = Vec::with_capacity(REQUIRED_MCP_CONFORMANCE_CHECKS.len());

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::Reset,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::AttachFixture,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    completed.push(McpConformanceCheck::FixtureAttached);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::CreateSession,
        McpConformanceObservation::SessionCreated,
    )
    .await?;
    completed.push(McpConformanceCheck::SessionCreated);

    let tools = execute(adapter, &transport, McpConformanceStep::ListTools).await?;
    match tools {
        McpConformanceObservation::ToolsVisible(names)
            if tools_are_bounded(&names)
                && names.iter().any(|name| name == MCP_CONFORMANCE_ECHO_TOOL)
                && names
                    .iter()
                    .any(|name| name == MCP_CONFORMANCE_MUTATING_TOOL) => {}
        _ => {
            return Err(McpConformanceFailure::UnexpectedObservation {
                transport,
                step: McpConformanceStep::ListTools,
            });
        }
    }
    completed.push(McpConformanceCheck::ToolsVisible);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::CallReadOnlyTool,
        McpConformanceObservation::ReadOnlyResult(MCP_CONFORMANCE_ECHO_VALUE.to_string()),
    )
    .await?;
    completed.push(McpConformanceCheck::ReadOnlyCall);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::RequestMutatingTool,
        McpConformanceObservation::ApprovalRequired {
            tool_name: MCP_CONFORMANCE_MUTATING_TOOL.to_string(),
        },
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::DenyMutatingTool,
        McpConformanceObservation::MutationDenied,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::ConfirmMutationNotExecuted,
        McpConformanceObservation::MutationNotExecuted,
    )
    .await?;
    completed.push(McpConformanceCheck::MutatingApproval);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::DisableFixture,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::RefreshAfterDisable,
        McpConformanceObservation::FixtureUnavailable,
    )
    .await?;
    completed.push(McpConformanceCheck::Disabled);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::RemoveFixture,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::ConfirmCleanup,
        McpConformanceObservation::CleanupConfirmed,
    )
    .await?;
    completed.push(McpConformanceCheck::Removed);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::AttachFixtureForServerFailure,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::MakeServerUnavailable,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::ConfirmServerFailure,
        McpConformanceObservation::FailedClosed(McpConformanceUnavailableKind::Server),
    )
    .await?;
    completed.push(McpConformanceCheck::ServerUnavailableFailsClosed);

    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::ResetAfterServerFailure,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::AttachFixtureForCredentialFailure,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::MakeCredentialUnavailable,
        McpConformanceObservation::Acknowledged,
    )
    .await?;
    expect_exact(
        adapter,
        &transport,
        McpConformanceStep::ConfirmCredentialFailure,
        McpConformanceObservation::FailedClosed(McpConformanceUnavailableKind::Credential),
    )
    .await?;
    completed.push(McpConformanceCheck::CredentialUnavailableFailsClosed);

    if completed != REQUIRED_MCP_CONFORMANCE_CHECKS {
        return Err(McpConformanceFailure::IncompleteCoverage { transport });
    }
    Ok(())
}

async fn execute<A>(
    adapter: &mut A,
    transport: &McpTransportKind,
    step: McpConformanceStep,
) -> Result<McpConformanceObservation, McpConformanceFailure>
where
    A: McpConformanceAdapter + ?Sized,
{
    adapter
        .execute(transport.clone(), step)
        .await
        .map_err(|category| McpConformanceFailure::Adapter {
            transport: transport.clone(),
            step,
            category,
        })
}

async fn expect_exact<A>(
    adapter: &mut A,
    transport: &McpTransportKind,
    step: McpConformanceStep,
    expected: McpConformanceObservation,
) -> Result<(), McpConformanceFailure>
where
    A: McpConformanceAdapter + ?Sized,
{
    let actual = execute(adapter, transport, step).await?;
    if actual != expected {
        return Err(McpConformanceFailure::UnexpectedObservation {
            transport: transport.clone(),
            step,
        });
    }
    Ok(())
}

fn tools_are_bounded(names: &[String]) -> bool {
    !names.is_empty()
        && names.len() <= MAX_VISIBLE_TOOLS
        && names
            .iter()
            .all(|name| !name.trim().is_empty() && name.chars().count() <= MAX_TOOL_NAME_CHARS)
}

fn validate_metadata(
    provider_id: &str,
    provider_version: &str,
) -> Result<(), McpConformanceFailure> {
    const MAX_METADATA_CHARS: usize = 128;
    if provider_id.trim().is_empty()
        || provider_version.trim().is_empty()
        || provider_id.chars().count() > MAX_METADATA_CHARS
        || provider_version.chars().count() > MAX_METADATA_CHARS
    {
        return Err(McpConformanceFailure::InvalidProviderMetadata);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum McpConformanceFailure {
    #[error("provider conformance metadata is invalid")]
    InvalidProviderMetadata,
    #[error("provider conformance adapter failed at {step:?} for {transport:?}: {category:?}")]
    Adapter {
        transport: McpTransportKind,
        step: McpConformanceStep,
        category: McpConformanceAdapterError,
    },
    #[error(
        "provider conformance returned an unexpected observation at {step:?} for {transport:?}"
    )]
    UnexpectedObservation {
        transport: McpTransportKind,
        step: McpConformanceStep,
    },
    #[error("provider conformance coverage is incomplete for {transport:?}")]
    IncompleteCoverage { transport: McpTransportKind },
    #[error("provider conformance evidence was rejected: {0}")]
    EvidenceRejected(McpConformanceEvidenceError),
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        domain::provider::{McpSupportLevel, ProviderId},
        ports::mcp_conformance::McpConformanceAdapterResult,
    };

    #[derive(Default)]
    struct OfflineFakeAdapter {
        calls: Vec<(McpTransportKind, McpConformanceStep)>,
        fail_at: Option<McpConformanceStep>,
        unexpected_at: Option<McpConformanceStep>,
    }

    #[async_trait]
    impl McpConformanceAdapter for OfflineFakeAdapter {
        fn provider_id(&self) -> ProviderId {
            ProviderId("offline-fixture-provider".to_string())
        }

        fn provider_version(&self) -> String {
            "1.0.0-test".to_string()
        }

        async fn execute(
            &mut self,
            transport: McpTransportKind,
            step: McpConformanceStep,
        ) -> McpConformanceAdapterResult<McpConformanceObservation> {
            self.calls.push((transport, step));
            if self.fail_at == Some(step) {
                return Err(McpConformanceAdapterError::Protocol);
            }
            if self.unexpected_at == Some(step) {
                return Ok(McpConformanceObservation::Acknowledged);
            }

            Ok(match step {
                McpConformanceStep::Reset
                | McpConformanceStep::AttachFixture
                | McpConformanceStep::DisableFixture
                | McpConformanceStep::RemoveFixture
                | McpConformanceStep::AttachFixtureForServerFailure
                | McpConformanceStep::MakeServerUnavailable
                | McpConformanceStep::ResetAfterServerFailure
                | McpConformanceStep::AttachFixtureForCredentialFailure
                | McpConformanceStep::MakeCredentialUnavailable => {
                    McpConformanceObservation::Acknowledged
                }
                McpConformanceStep::CreateSession => McpConformanceObservation::SessionCreated,
                McpConformanceStep::ListTools => McpConformanceObservation::ToolsVisible(vec![
                    MCP_CONFORMANCE_ECHO_TOOL.to_string(),
                    MCP_CONFORMANCE_MUTATING_TOOL.to_string(),
                ]),
                McpConformanceStep::CallReadOnlyTool => McpConformanceObservation::ReadOnlyResult(
                    MCP_CONFORMANCE_ECHO_VALUE.to_string(),
                ),
                McpConformanceStep::RequestMutatingTool => {
                    McpConformanceObservation::ApprovalRequired {
                        tool_name: MCP_CONFORMANCE_MUTATING_TOOL.to_string(),
                    }
                }
                McpConformanceStep::DenyMutatingTool => McpConformanceObservation::MutationDenied,
                McpConformanceStep::ConfirmMutationNotExecuted => {
                    McpConformanceObservation::MutationNotExecuted
                }
                McpConformanceStep::RefreshAfterDisable => {
                    McpConformanceObservation::FixtureUnavailable
                }
                McpConformanceStep::ConfirmCleanup => McpConformanceObservation::CleanupConfirmed,
                McpConformanceStep::FinalCleanup => McpConformanceObservation::CleanupConfirmed,
                McpConformanceStep::ConfirmServerFailure => {
                    McpConformanceObservation::FailedClosed(McpConformanceUnavailableKind::Server)
                }
                McpConformanceStep::ConfirmCredentialFailure => {
                    McpConformanceObservation::FailedClosed(
                        McpConformanceUnavailableKind::Credential,
                    )
                }
            })
        }
    }

    #[test]
    fn both_transports_must_pass_before_verified_evidence_exists() {
        let mut adapter = OfflineFakeAdapter::default();
        let evidence = futures::executor::block_on(run_provider_mcp_conformance(&mut adapter))
            .expect("offline conformance");

        evidence.validate().expect("valid evidence");
        assert_eq!(evidence.provider_version(), "1.0.0-test");
        assert_eq!(evidence.transports().len(), 2);
        let support = McpSupportLevel::VerifiedBridge { evidence };
        support.validate().expect("verified support");
        assert!(support.verified_evidence().is_some());
        assert!(adapter
            .calls
            .iter()
            .any(|(transport, _)| transport == &McpTransportKind::Stdio));
        assert!(adapter
            .calls
            .iter()
            .any(|(transport, _)| transport == &McpTransportKind::Http));
    }

    #[test]
    fn one_failed_behavior_produces_no_evidence() {
        let mut adapter = OfflineFakeAdapter {
            fail_at: Some(McpConformanceStep::RequestMutatingTool),
            ..Default::default()
        };

        let error = futures::executor::block_on(run_provider_mcp_conformance(&mut adapter))
            .expect_err("approval failure must fail conformance");
        assert_eq!(
            error,
            McpConformanceFailure::Adapter {
                transport: McpTransportKind::Stdio,
                step: McpConformanceStep::RequestMutatingTool,
                category: McpConformanceAdapterError::Protocol,
            }
        );
        assert!(adapter
            .calls
            .contains(&(McpTransportKind::Stdio, McpConformanceStep::FinalCleanup)));
        assert!(!error.to_string().contains("payload"));
    }

    #[test]
    fn fail_closed_is_an_observed_result_not_a_generic_success() {
        let mut adapter = OfflineFakeAdapter {
            unexpected_at: Some(McpConformanceStep::ConfirmCredentialFailure),
            ..Default::default()
        };

        let error = futures::executor::block_on(run_provider_mcp_conformance(&mut adapter))
            .expect_err("missing fail-closed observation");
        assert_eq!(
            error,
            McpConformanceFailure::UnexpectedObservation {
                transport: McpTransportKind::Stdio,
                step: McpConformanceStep::ConfirmCredentialFailure,
            }
        );
    }
}
