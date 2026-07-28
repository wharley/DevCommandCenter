mod http;
mod protocol;
mod stdio;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use dcc_core::{
    domain::mcp::{McpDefinition, McpErrorCategory, McpProbeReport, McpRuntimeError},
    ports::{CredentialStore, McpProbe, McpProbeResult},
};

#[derive(Clone, Debug)]
pub struct McpProbeLimits {
    pub initialize_timeout: Duration,
    pub list_tools_timeout: Duration,
    pub shutdown_timeout: Duration,
    pub max_response_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_tools: usize,
    pub max_tool_name_chars: usize,
}

impl Default for McpProbeLimits {
    fn default() -> Self {
        Self {
            initialize_timeout: Duration::from_secs(5),
            list_tools_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(1),
            max_response_bytes: 256 * 1024,
            max_stderr_bytes: 8 * 1024,
            max_tools: 256,
            max_tool_name_chars: 128,
        }
    }
}

pub struct SecureMcpProbe<C: ?Sized> {
    credentials: Arc<C>,
    http_client: reqwest::Client,
    limits: McpProbeLimits,
}

impl<C> SecureMcpProbe<C>
where
    C: CredentialStore + ?Sized,
{
    pub fn new(credentials: Arc<C>) -> McpProbeResult<Self> {
        Self::with_limits(credentials, McpProbeLimits::default())
    }

    pub fn with_limits(credentials: Arc<C>, limits: McpProbeLimits) -> McpProbeResult<Self> {
        validate_limits(&limits)?;
        let http_client = reqwest::Client::builder()
            .connect_timeout(limits.initialize_timeout)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("dcc-mcp-probe/0.1")
            .build()
            .map_err(|_| {
                probe_error(
                    McpErrorCategory::Transport,
                    "failed to initialize the MCP HTTP probe",
                )
            })?;
        Ok(Self {
            credentials,
            http_client,
            limits,
        })
    }
}

#[async_trait]
impl<C> McpProbe for SecureMcpProbe<C>
where
    C: CredentialStore + ?Sized,
{
    async fn probe(&self, definition: &McpDefinition) -> McpProbeResult<McpProbeReport> {
        definition.validate().map_err(|_| {
            probe_error(
                McpErrorCategory::InvalidDefinition,
                "MCP definition is invalid",
            )
        })?;
        if definition.trust.requires_confirmation() {
            return Err(probe_error(
                McpErrorCategory::PermissionBoundary,
                "MCP definition requires explicit trust before probing",
            ));
        }

        match &definition.transport {
            dcc_core::domain::mcp::McpTransport::Stdio { .. } => {
                stdio::probe_stdio(self, definition).await
            }
            dcc_core::domain::mcp::McpTransport::Http { .. } => {
                http::probe_http(self, definition).await
            }
        }
    }
}

fn validate_limits(limits: &McpProbeLimits) -> McpProbeResult<()> {
    if limits.initialize_timeout.is_zero()
        || limits.initialize_timeout > Duration::from_secs(60)
        || limits.list_tools_timeout.is_zero()
        || limits.list_tools_timeout > Duration::from_secs(60)
        || limits.shutdown_timeout.is_zero()
        || limits.shutdown_timeout > Duration::from_secs(10)
        || !(1024..=4 * 1024 * 1024).contains(&limits.max_response_bytes)
        || !(256..=64 * 1024).contains(&limits.max_stderr_bytes)
        || !(1..=256).contains(&limits.max_tools)
        || !(1..=128).contains(&limits.max_tool_name_chars)
    {
        return Err(probe_error(
            McpErrorCategory::InvalidDefinition,
            "MCP probe limits are invalid",
        ));
    }
    Ok(())
}

fn probe_error(category: McpErrorCategory, message: &'static str) -> McpRuntimeError {
    McpRuntimeError::bounded(category, message)
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::mcp::McpErrorCategory;

    use super::*;
    use crate::credential_store::InMemoryCredentialStore;

    #[test]
    fn custom_limits_remain_inside_hard_safety_ceilings() {
        let limits = McpProbeLimits {
            initialize_timeout: Duration::from_secs(61),
            ..McpProbeLimits::default()
        };

        let error =
            SecureMcpProbe::with_limits(Arc::new(InMemoryCredentialStore::default()), limits)
                .err()
                .expect("oversized timeout must fail");
        assert_eq!(error.category, McpErrorCategory::InvalidDefinition);
    }
}
