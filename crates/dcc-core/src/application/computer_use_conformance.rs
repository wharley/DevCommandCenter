use chrono::Utc;
use thiserror::Error;

use crate::{
    domain::{
        computer_use_conformance::{
            ComputerUseConformanceEvidence, ComputerUseConformanceEvidenceError,
        },
        mcp::McpTransportKind,
    },
    ports::computer_use_conformance::{
        ComputerUseConformanceAdapter, ComputerUseConformanceAdapterError,
        ComputerUseConformanceObservation, ComputerUseConformanceStep,
    },
};

/// Runs the Computer Use conformance gate over both MCP transports. It is
/// deliberately independent from the MCP bridge gate and returns evidence
/// only after image understanding and active-turn interruption both pass.
pub async fn run_provider_computer_use_conformance<A>(
    adapter: &mut A,
) -> Result<ComputerUseConformanceEvidence, ComputerUseConformanceFailure>
where
    A: ComputerUseConformanceAdapter + ?Sized,
{
    let provider_id = adapter.provider_id();
    let provider_version = adapter.provider_version();
    validate_metadata(&provider_id.0, &provider_version)?;

    for transport in [McpTransportKind::Stdio, McpTransportKind::Http] {
        let result = run_transport(adapter, transport.clone()).await;
        let cleanup = expect(
            adapter,
            &transport,
            ComputerUseConformanceStep::FinalCleanup,
            ComputerUseConformanceObservation::CleanupConfirmed,
        )
        .await;
        match result {
            Err(error) => return Err(error),
            Ok(()) => cleanup?,
        }
    }

    ComputerUseConformanceEvidence::from_successful_run(
        provider_id,
        provider_version,
        Utc::now().to_rfc3339(),
    )
    .map_err(ComputerUseConformanceFailure::EvidenceRejected)
}

async fn run_transport<A>(
    adapter: &mut A,
    transport: McpTransportKind,
) -> Result<(), ComputerUseConformanceFailure>
where
    A: ComputerUseConformanceAdapter + ?Sized,
{
    expect(
        adapter,
        &transport,
        ComputerUseConformanceStep::Reset,
        ComputerUseConformanceObservation::Acknowledged,
    )
    .await?;
    expect(
        adapter,
        &transport,
        ComputerUseConformanceStep::AttachFixture,
        ComputerUseConformanceObservation::Acknowledged,
    )
    .await?;
    expect(
        adapter,
        &transport,
        ComputerUseConformanceStep::CreateSession,
        ComputerUseConformanceObservation::SessionCreated,
    )
    .await?;
    let image_pass = expect(
        adapter,
        &transport,
        ComputerUseConformanceStep::InspectImage,
        ComputerUseConformanceObservation::ImageUnderstood,
    )
    .await
    .is_ok();
    let hold_started = expect(
        adapter,
        &transport,
        ComputerUseConformanceStep::StartInterruptibleOperation,
        ComputerUseConformanceObservation::Acknowledged,
    )
    .await
    .is_ok();
    let cancel_requested = if hold_started {
        let cancel_requested = expect(
            adapter,
            &transport,
            ComputerUseConformanceStep::InterruptTurn,
            ComputerUseConformanceObservation::InterruptRequested,
        )
        .await
        .is_ok();
        cancel_requested
    } else {
        false
    };
    let turn_stopped = if cancel_requested {
        expect(
            adapter,
            &transport,
            ComputerUseConformanceStep::ConfirmTurnStopped,
            ComputerUseConformanceObservation::TurnStopped,
        )
        .await
        .is_ok()
    } else {
        false
    };
    let interruption_pass = hold_started && cancel_requested && turn_stopped;
    if !image_pass || !interruption_pass {
        return Err(ComputerUseConformanceFailure::ChecksFailed {
            transport,
            image_pass,
            hold_started,
            cancel_requested,
            turn_stopped,
        });
    }
    Ok(())
}

async fn expect<A>(
    adapter: &mut A,
    transport: &McpTransportKind,
    step: ComputerUseConformanceStep,
    expected: ComputerUseConformanceObservation,
) -> Result<(), ComputerUseConformanceFailure>
where
    A: ComputerUseConformanceAdapter + ?Sized,
{
    let observation = adapter
        .execute(transport.clone(), step)
        .await
        .map_err(|category| ComputerUseConformanceFailure::Adapter {
            transport: transport.clone(),
            step,
            category,
        })?;
    if observation != expected {
        return Err(ComputerUseConformanceFailure::UnexpectedObservation {
            transport: transport.clone(),
            step,
        });
    }
    Ok(())
}

fn validate_metadata(
    provider_id: &str,
    provider_version: &str,
) -> Result<(), ComputerUseConformanceFailure> {
    if [provider_id, provider_version].iter().any(|value| {
        value.trim().is_empty()
            || value.chars().count() > 128
            || value.chars().any(char::is_control)
    }) {
        return Err(ComputerUseConformanceFailure::InvalidProviderMetadata);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ComputerUseConformanceFailure {
    #[error("Computer Use conformance provider metadata is invalid")]
    InvalidProviderMetadata,
    #[error("Computer Use conformance evidence was rejected")]
    EvidenceRejected(ComputerUseConformanceEvidenceError),
    #[error("Computer Use provider adapter failed at {step:?} for {transport:?}: {category:?}")]
    Adapter {
        transport: McpTransportKind,
        step: ComputerUseConformanceStep,
        category: ComputerUseConformanceAdapterError,
    },
    #[error(
        "Computer Use provider returned an unexpected observation at {step:?} for {transport:?}"
    )]
    UnexpectedObservation {
        transport: McpTransportKind,
        step: ComputerUseConformanceStep,
    },
    #[error("Computer Use provider checks failed for {transport:?}: image_pass={image_pass}, hold_started={hold_started}, cancel_requested={cancel_requested}, turn_stopped={turn_stopped}")]
    ChecksFailed {
        transport: McpTransportKind,
        image_pass: bool,
        hold_started: bool,
        cancel_requested: bool,
        turn_stopped: bool,
    },
}
