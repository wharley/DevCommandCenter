//! Connector half of DCC's loopback MCP gateway. The provider authenticates
//! to DCC with its session bearer; this module swaps in the vendor token,
//! forwards the call, and keeps the provider unaware of sign-ins.
//!
//! The gateway never answers HTTP 401: a provider's MCP client reads that as
//! "sign in to this server" and would start OAuth against DCC. A lost sign-in
//! becomes a JSON-RPC error the model can relay, plus a `needs_auth` signal
//! for the UI.

use std::sync::Arc;

use chrono::Utc;
use dcc_core::domain::mcp::{
    McpConnectionCheck, McpConnectionState, McpDefinition, McpDefinitionId, McpToolAnnotations,
    McpToolSummary, McpTransport,
};
use dcc_infra::{
    mcp_gateway::{ConnectorUpstream, UpstreamOutcome, UpstreamRequest},
    mcp_oauth::McpOauthError,
};
use serde_json::{json, Value};
use url::Url;

use crate::mcp_connect::{ConnectStores, McpConnectCoordinator};

const GATEWAY_ERROR: i64 = -32001;
const CHECK_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_TOOL_PAGES: u64 = 10;
const MAX_TOOLS: usize = 256;

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
            Err(_) => {
                return error_response(&call, "DCC could not read this connector. Try again shortly.")
            }
        };
        let outcome = self
            .exchange(
                &definition,
                call.session_id.as_deref(),
                call.protocol_version.as_deref(),
                &call.body,
            )
            .await;
        match outcome {
            Ok(UpstreamOutcome::Reply {
                session_id,
                body: Some(body),
            }) => ConnectorResponse::Json { session_id, body },
            Ok(UpstreamOutcome::Reply {
                session_id,
                body: None,
            }) => ConnectorResponse::Accepted { session_id },
            Ok(UpstreamOutcome::SessionExpired) => ConnectorResponse::SessionExpired,
            Ok(UpstreamOutcome::Failed(message)) => error_response(&call, &message),
            Ok(UpstreamOutcome::Unauthorized) => unreachable!("exchange resolves 401s"),
            Err(error) => error_response(&call, &self.auth_failure_text(&definition, &error).await),
        }
    }

    /// Initializes a short MCP session with the vendor, lists its tools, and
    /// records the result as the connector's last connection check.
    pub async fn check(&self, definition: &McpDefinition) -> McpConnectionCheck {
        let result = self.list_tools(definition).await;
        let check = match result {
            Ok(tools) => connection_check(definition, McpConnectionState::Connected, tools, None),
            Err(CheckFailure::Auth(error)) if error.needs_reauthorization() => {
                return self.mark_needs_auth(definition).await
            }
            Err(CheckFailure::Auth(error)) => connection_check(
                definition,
                McpConnectionState::Failed,
                Vec::new(),
                Some(error.to_string()),
            ),
            Err(CheckFailure::Server(message)) => connection_check(
                definition,
                McpConnectionState::Failed,
                Vec::new(),
                Some(message),
            ),
        };
        let _ = self.stores.repo.save_mcp_connection_check(&check).await;
        check
    }

    async fn list_tools(
        &self,
        definition: &McpDefinition,
    ) -> Result<Vec<McpToolSummary>, CheckFailure> {
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": CHECK_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "dcc-connection-check", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let (session_id, body) = self.request(definition, None, None, &initialize).await?;
        let result = rpc_result(&body)?;
        let protocol = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(CHECK_PROTOCOL_VERSION)
            .to_string();
        let session = session_id.as_deref();
        let listed = async {
            let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
            self.exchange(definition, session, Some(&protocol), &initialized)
                .await
                .map_err(CheckFailure::Auth)?;
            let mut tools = Vec::new();
            let mut cursor: Option<String> = None;
            for page in 0..MAX_TOOL_PAGES {
                let mut params = json!({});
                if let Some(cursor) = &cursor {
                    params["cursor"] = json!(cursor);
                }
                let request = json!({"jsonrpc": "2.0", "id": 2 + page, "method": "tools/list", "params": params});
                let (_, body) = self
                    .request(definition, session, Some(&protocol), &request)
                    .await?;
                let result = rpc_result(&body)?;
                collect_tools(&result, &mut tools);
                cursor = result
                    .get("nextCursor")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if cursor.is_none() || tools.len() >= MAX_TOOLS {
                    break;
                }
            }
            Ok(tools)
        }
        .await;
        if let Some(session_id) = session {
            self.close_definition_session(definition, session_id).await;
        }
        listed
    }

    async fn request(
        &self,
        definition: &McpDefinition,
        session_id: Option<&str>,
        protocol_version: Option<&str>,
        body: &Value,
    ) -> Result<(Option<String>, Value), CheckFailure> {
        match self
            .exchange(definition, session_id, protocol_version, body)
            .await
            .map_err(CheckFailure::Auth)?
        {
            UpstreamOutcome::Reply {
                session_id,
                body: Some(body),
            } => Ok((session_id, body)),
            UpstreamOutcome::Failed(message) => Err(CheckFailure::Server(message)),
            _ => Err(CheckFailure::Server(
                "The server did not answer the MCP request".to_string(),
            )),
        }
    }

    /// Sends one exchange with a valid vendor token: one forced refresh and
    /// one retry on 401. `Err` means the sign-in itself is the problem.
    async fn exchange(
        &self,
        definition: &McpDefinition,
        session_id: Option<&str>,
        protocol_version: Option<&str>,
        body: &Value,
    ) -> Result<UpstreamOutcome, McpOauthError> {
        let McpTransport::Http { url } = &definition.transport else {
            return Ok(UpstreamOutcome::Failed(
                "This connector is not a remote server".to_string(),
            ));
        };
        let Ok(url) = Url::parse(url) else {
            return Ok(UpstreamOutcome::Failed(
                "This connector has an invalid URL".to_string(),
            ));
        };
        let mut authorization = self.authorization_header(definition, false).await?;
        for attempt in 0..2 {
            let outcome = self
                .upstream
                .forward(UpstreamRequest {
                    url: &url,
                    authorization: Some(&authorization),
                    session_id,
                    protocol_version,
                    body,
                })
                .await;
            if outcome != UpstreamOutcome::Unauthorized {
                return Ok(outcome);
            }
            if attempt == 0 {
                authorization = self.authorization_header(definition, true).await?;
            }
        }
        Err(McpOauthError::ReauthorizationRequired)
    }

    /// Ends the provider's MCP session upstream (`DELETE`), best effort.
    pub async fn close(&self, definition_id: &McpDefinitionId, session_id: &str) {
        if let Ok(Some(definition)) = self.stores.repo.get_mcp_definition(definition_id).await {
            self.close_definition_session(&definition, session_id).await;
        }
    }

    async fn close_definition_session(&self, definition: &McpDefinition, session_id: &str) {
        let McpTransport::Http { url } = &definition.transport else {
            return;
        };
        let Ok(url) = Url::parse(url) else {
            return;
        };
        let authorization = self.authorization_header(definition, false).await.ok();
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

    /// Records `NeedsAuth` (keeping the last known tools) and tells the UI.
    async fn mark_needs_auth(&self, definition: &McpDefinition) -> McpConnectionCheck {
        let previous_tools = self
            .stores
            .repo
            .list_mcp_connection_checks()
            .await
            .ok()
            .and_then(|checks| {
                checks
                    .into_iter()
                    .find(|check| check.definition_id == definition.id)
            })
            .map(|check| check.tools)
            .unwrap_or_default();
        let check = connection_check(
            definition,
            McpConnectionState::NeedsAuth,
            previous_tools,
            Some("The sign-in expired. Reconnect to keep using it".to_string()),
        );
        let _ = self.stores.repo.save_mcp_connection_check(&check).await;
        (self.on_needs_auth)(&definition.id);
        check
    }

    async fn auth_failure_text(&self, definition: &McpDefinition, error: &McpOauthError) -> String {
        if error.needs_reauthorization() {
            self.mark_needs_auth(definition).await;
            return format!(
                "{} needs to be reconnected. Ask the user to reconnect it in DCC under Settings → Connectors.",
                definition.display_name
            );
        }
        format!(
            "{} is temporarily unavailable ({error}). Try again shortly.",
            definition.display_name
        )
    }
}

enum CheckFailure {
    Auth(McpOauthError),
    Server(String),
}

fn connection_check(
    definition: &McpDefinition,
    state: McpConnectionState,
    tools: Vec<McpToolSummary>,
    message: Option<String>,
) -> McpConnectionCheck {
    McpConnectionCheck {
        definition_id: definition.id.clone(),
        state,
        tools,
        message: message.map(|message| message.chars().take(500).collect()),
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn rpc_result(body: &Value) -> Result<Value, CheckFailure> {
    if let Some(result) = body.get("result") {
        return Ok(result.clone());
    }
    let message = body
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("The server returned an invalid MCP response");
    Err(CheckFailure::Server(message.chars().take(300).collect()))
}

fn collect_tools(result: &Value, tools: &mut Vec<McpToolSummary>) {
    let Some(listed) = result.get("tools").and_then(Value::as_array) else {
        return;
    };
    for tool in listed {
        let Some(name) = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty() && name.chars().count() <= 128)
        else {
            continue;
        };
        if tools.len() >= MAX_TOOLS || tools.iter().any(|known| known.name == name) {
            continue;
        }
        let hint = |field: &str| {
            tool.get("annotations")
                .and_then(|annotations| annotations.get(field))
                .and_then(Value::as_bool)
        };
        tools.push(McpToolSummary {
            name: name.to_string(),
            annotations: McpToolAnnotations {
                read_only_hint: hint("readOnlyHint"),
                destructive_hint: hint("destructiveHint"),
                idempotent_hint: hint("idempotentHint"),
                open_world_hint: hint("openWorldHint"),
            },
        });
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
