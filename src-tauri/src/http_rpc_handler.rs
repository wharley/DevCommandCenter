use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::daemon_client::rpc_with_db_path_timeout;
use crate::http_config::HttpConfig;

/// RPC request from HTTP client
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// RPC response to HTTP client
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Handler for POST /rpc endpoint
pub async fn handle_rpc(
    State(config): State<Arc<HttpConfig>>,
    Json(request): Json<RpcRequest>,
) -> Result<Json<RpcResponse>, RpcError> {
    // Validate method against whitelist
    if !is_valid_method(&request.method) {
        return Ok(Json(RpcResponse {
            ok: false,
            result: None,
            error: Some(format!("Invalid method: {}", request.method)),
        }));
    }

    // Call RPC via SQLite (reusing existing daemon client!)
    match rpc_with_db_path_timeout(
        &config.db_path,
        &request.method,
        request.params,
        Duration::from_secs(30),
    ) {
        Ok(result) => Ok(Json(RpcResponse {
            ok: true,
            result: Some(result),
            error: None,
        })),
        Err(err) => {
            // Check if daemon is not running
            if err.contains("no such table") || err.contains("unable to open database") {
                return Err(RpcError::DaemonNotRunning);
            }

            // Return error in response body (not HTTP error)
            Ok(Json(RpcResponse {
                ok: false,
                result: None,
                error: Some(err),
            }))
        }
    }
}

/// Whitelist of methods allowed via HTTP
fn is_valid_method(method: &str) -> bool {
    matches!(
        method,
        "daemon.health"
            | "daemon.getStatus"
            | "daemon.listTasks"
            | "daemon.runTask"
            | "daemon.attachTask"
            | "daemon.detachTask"
            | "daemon.listProcesses"
            | "daemon.startProcess"
            | "daemon.stopProcess"
            | "daemon.restartProcess"
            | "combs.list"
            | "panes.list"
            | "diffs.bundle"
    )
}

/// Errors that can occur during RPC handling
#[derive(Debug)]
pub enum RpcError {
    DaemonNotRunning,
    Timeout,
    Internal(String),
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            RpcError::DaemonNotRunning => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Daemon not running",
            ),
            RpcError::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "Daemon timeout",
            ),
            RpcError::Internal(ref msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                msg.as_str(),
            ),
        };

        (
            status,
            Json(serde_json::json!({
                "ok": false,
                "error": message
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_method() {
        assert!(is_valid_method("daemon.health"));
        assert!(is_valid_method("daemon.getStatus"));
        assert!(is_valid_method("daemon.listTasks"));
        assert!(is_valid_method("combs.list"));
        assert!(!is_valid_method("daemon.deleteEverything"));
        assert!(!is_valid_method("unknown.method"));
    }

    #[test]
    fn test_rpc_request_deserialization() {
        let json = r#"{"method":"daemon.health","params":{}}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "daemon.health");
    }

    #[test]
    fn test_rpc_response_serialization() {
        let resp = RpcResponse {
            ok: true,
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"ok\":true"));
    }
}
