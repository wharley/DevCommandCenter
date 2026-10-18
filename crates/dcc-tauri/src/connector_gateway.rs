//! Connector half of DCC's loopback MCP gateway. The provider authenticates
//! to DCC with its session bearer; this module swaps in the vendor token,
//! forwards the call, and keeps the provider unaware of sign-ins.
//!
//! The gateway never answers HTTP 401: a provider's MCP client reads that as
//! "sign in to this server" and would start OAuth against DCC. A lost sign-in
//! becomes a JSON-RPC error the model can relay, plus a `needs_auth` signal
//! for the UI.

use std::sync::Arc;

use dcc_core::domain::mcp::{McpDefinition, McpDefinitionId, McpTransport};
use dcc_infra::{
    mcp_gateway::{ConnectorUpstream, UpstreamOutcome, UpstreamRequest},
    mcp_oauth::McpOauthError,
};
use serde_json::{json, Value};
use url::Url;

use crate::mcp_connect::{ConnectStores, McpConnectCoordinator};

const GATEWAY_ERROR: i64 = -32001;

pub type NeedsAuthCallback = Arc<dyn Fn(&McpDefinitionId) + Send + Sync>;

pub struct ConnectorCall {
    pub session_id: Option<String>,
    pub protocol_version: Option<String>,
    pub body: Value,
}

#[derive(Debug, PartialEq)]
pub enum ConnectorResponse {
    Json {
        session_id: Option<String>,
        body: Value,
    },
    Accepted {
        session_id: Option<String>,
    },
    /// HTTP 404: the client must initialize a new MCP session.
    SessionExpired,
}

#[derive(Clone)]
pub struct ConnectorGateway {
    coordinator: McpConnectCoordinator,
    upstream: ConnectorUpstream,
    stores: ConnectStores,
    on_needs_auth: NeedsAuthCallback,
}

impl ConnectorGateway {
    pub fn new(
        coordinator: McpConnectCoordinator,
        stores: ConnectStores,
        on_needs_auth: NeedsAuthCallback,
    ) -> Result<Self, String> {
        Ok(Self {
            coordinator,
            upstream: ConnectorUpstream::new()?,
            stores,
            on_needs_auth,
        })
    }

    pub async fn handle(
        &self,
        definition_id: &McpDefinitionId,
        call: ConnectorCall,
    ) -> ConnectorResponse {
        let definition = match self.stores.repo.get_mcp_definition(definition_id).await {
            Ok(Some(definition)) if definition.enabled => definition,
            Ok(_) => {
                return error_response(
                    &call,
                    "This connector was disconnected in DCC. Start a new conversation to use the connectors that are connected now.",
                )
            }
            Err(_) => return error_response(&call, "DCC could not read this connector. Try again shortly."),
        };
        let McpTransport::Http { url } = &definition.transport else {
            return error_response(&call, "This connector is not a remote server.");
        };
        let Ok(url) = Url::parse(url) else {
            return error_response(&call, "This connector has an invalid URL.");
        };

        let mut authorization = match self.authorization_header(&definition, false).await {
            Ok(authorization) => authorization,
            Err(error) => return self.auth_error(&definition, &call, &error),
        };
        for attempt in 0..2 {
            let outcome = self
                .upstream
                .forward(UpstreamRequest {
                    url: &url,
                    authorization: Some(&authorization),
                    session_id: call.session_id.as_deref(),
                    protocol_version: call.protocol_version.as_deref(),
                    body: &call.body,
                })
                .await;
            match outcome {
                UpstreamOutcome::Reply {
                    session_id,
                    body: Some(body),
                } => return ConnectorResponse::Json { session_id, body },
                UpstreamOutcome::Reply {
                    session_id,
                    body: None,
                } => return ConnectorResponse::Accepted { session_id },
                UpstreamOutcome::SessionExpired => return ConnectorResponse::SessionExpired,
                UpstreamOutcome::Failed(message) => return error_response(&call, &message),
                UpstreamOutcome::Unauthorized => {
                    if attempt == 1 {
                        break;
                    }
                    // One forced refresh, then one retry.
                    authorization = match self.authorization_header(&definition, true).await {
                        Ok(authorization) => authorization,
                        Err(error) => return self.auth_error(&definition, &call, &error),
                    };
                }
            }
        }
        self.auth_error(&definition, &call, &McpOauthError::ReauthorizationRequired)
    }

    /// Ends the provider's MCP session upstream (`DELETE`), best effort.
    pub async fn close(&self, definition_id: &McpDefinitionId, session_id: &str) {
        let Ok(Some(definition)) = self.stores.repo.get_mcp_definition(definition_id).await else {
            return;
        };
        let McpTransport::Http { url } = &definition.transport else {
            return;
        };
        let Ok(url) = Url::parse(url) else {
            return;
        };
        let authorization = self.authorization_header(&definition, false).await.ok();
        self.upstream
            .close_session(&url, authorization.as_deref(), session_id)
            .await;
    }

    /// Only signed-in connectors are served here, so a missing grant (the
    /// user disconnected mid-session) means the connector needs a sign-in.
    async fn authorization_header(
        &self,
        definition: &McpDefinition,
        force_refresh: bool,
    ) -> Result<String, McpOauthError> {
        Ok(self
            .coordinator
            .access_credential(&self.stores, definition, force_refresh)
            .await?
            .authorization_header())
    }

    fn auth_error(
        &self,
        definition: &McpDefinition,
        call: &ConnectorCall,
        error: &McpOauthError,
    ) -> ConnectorResponse {
        if error.needs_reauthorization() {
            (self.on_needs_auth)(&definition.id);
            return error_response(
                call,
                &format!(
                    "{} needs to be reconnected. Ask the user to reconnect it in DCC under Settings → Connectors.",
                    definition.display_name
                ),
            );
        }
        error_response(
            call,
            &format!(
                "{} is temporarily unavailable ({error}). Try again shortly.",
                definition.display_name
            ),
        )
    }
}

/// A JSON-RPC error for every request id in the call; notifications are
/// simply accepted.
fn error_response(call: &ConnectorCall, message: &str) -> ConnectorResponse {
    let error_for = |id: &Value| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": GATEWAY_ERROR, "message": message },
        })
    };
    let requests = |message: &Value| {
        message.get("method").is_some() && message.get("id").is_some_and(|id| !id.is_null())
    };
    let body = match &call.body {
        Value::Array(items) => {
            let errors = items
                .iter()
                .filter(|item| requests(item))
                .map(|item| error_for(&item["id"]))
                .collect::<Vec<_>>();
            (!errors.is_empty()).then_some(Value::Array(errors))
        }
        message if requests(message) => Some(error_for(&message["id"])),
        _ => None,
    };
    match body {
        Some(body) => ConnectorResponse::Json {
            session_id: None,
            body,
        },
        None => ConnectorResponse::Accepted { session_id: None },
    }
}
