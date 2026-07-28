pub mod http;
pub mod stdio;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde_json::{json, Value};
use tokio::sync::{broadcast, Mutex, Notify, Semaphore};

pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &[LATEST_PROTOCOL_VERSION, "2025-06-18", "2025-03-26"];
pub const MAX_ECHO_CHARS: usize = 4_096;
pub const MAX_SLOW_DELAY_MS: u64 = 2_000;
pub const MAX_CONCURRENT_REQUESTS: usize = 32;

const JSON_RPC_VERSION: &str = "2.0";
const REQUEST_CANCELLED: i64 = -32_800;
const INVALID_REQUEST: i64 = -32_600;
const METHOD_NOT_FOUND: i64 = -32_601;
const INVALID_PARAMS: i64 = -32_602;
const SERVER_BUSY: i64 = -32_000;

#[derive(Clone)]
pub struct FixtureServer {
    inner: Arc<FixtureState>,
}

struct FixtureState {
    mutation_count: AtomicU64,
    dynamic_tool_enabled: AtomicBool,
    notifications: broadcast::Sender<Value>,
    in_flight: Mutex<HashMap<RequestKey, Arc<Notify>>>,
    request_slots: Arc<Semaphore>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum RequestKey {
    Number(String),
    String(String),
}

impl Default for FixtureServer {
    fn default() -> Self {
        Self::new()
    }
}

impl FixtureServer {
    pub fn new() -> Self {
        let (notifications, _) = broadcast::channel(32);
        Self {
            inner: Arc::new(FixtureState {
                mutation_count: AtomicU64::new(0),
                dynamic_tool_enabled: AtomicBool::new(false),
                notifications,
                in_flight: Mutex::new(HashMap::new()),
                request_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.inner.notifications.subscribe()
    }

    pub async fn handle_message(&self, message: Value) -> Option<Value> {
        if !message.is_object()
            || message.get("jsonrpc").and_then(Value::as_str) != Some(JSON_RPC_VERSION)
        {
            return Some(error_response(
                message.get("id").cloned().unwrap_or(Value::Null),
                INVALID_REQUEST,
                "Invalid Request",
            ));
        }

        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Some(error_response(
                message.get("id").cloned().unwrap_or(Value::Null),
                INVALID_REQUEST,
                "Invalid Request",
            ));
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        let Some(id) = message.get("id").cloned() else {
            self.handle_notification(method, params).await;
            return None;
        };
        if request_key(&id).is_none() {
            return Some(error_response(id, INVALID_REQUEST, "Invalid Request"));
        }
        let Ok(_permit) = self.inner.request_slots.clone().try_acquire_owned() else {
            return Some(error_response(
                id,
                SERVER_BUSY,
                "Fixture request limit reached",
            ));
        };

        Some(self.handle_request(id, method, params).await)
    }

    async fn handle_notification(&self, method: &str, params: Value) {
        if method != "notifications/cancelled" {
            return;
        }

        let Some(request_id) = params.get("requestId").and_then(request_key) else {
            return;
        };
        let cancellation = self.inner.in_flight.lock().await.get(&request_id).cloned();
        if let Some(cancellation) = cancellation {
            cancellation.notify_waiters();
        }
    }

    async fn handle_request(&self, id: Value, method: &str, params: Value) -> Value {
        match method {
            "initialize" => match initialize_result(&params) {
                Some(result) => success_response(id, result),
                None => error_response(id, INVALID_PARAMS, "Invalid initialize parameters"),
            },
            "ping" => success_response(id, json!({})),
            "tools/list" => success_response(id, json!({ "tools": self.tools() })),
            "tools/call" => self.call_tool(id, params).await,
            _ => error_response(id, METHOD_NOT_FOUND, "Method not found"),
        }
    }

    async fn call_tool(&self, id: Value, params: Value) -> Value {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return error_response(id, INVALID_PARAMS, "Tool name is required");
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return tool_error(id, "Tool arguments must be an object");
        }

        match name {
            "fixture.echo" => self.echo(id, &arguments),
            "fixture.mutate" => self.mutate(id, &arguments),
            "fixture.slow" => self.slow(id, &arguments).await,
            "fixture.fail" => {
                if has_unknown_arguments(&arguments, &[]) {
                    tool_error(id, "fixture.fail does not accept arguments")
                } else {
                    tool_error(id, "Deterministic fixture failure")
                }
            }
            "fixture.malformed_result" => {
                if has_unknown_arguments(&arguments, &[]) {
                    tool_error(id, "fixture.malformed_result does not accept arguments")
                } else {
                    success_response(
                        id,
                        json!({
                            "content": "intentionally-not-an-array",
                            "isError": false
                        }),
                    )
                }
            }
            "fixture.dynamic" if self.inner.dynamic_tool_enabled.load(Ordering::SeqCst) => {
                if has_unknown_arguments(&arguments, &[]) {
                    tool_error(id, "fixture.dynamic does not accept arguments")
                } else {
                    success_response(
                        id,
                        json!({
                            "content": [{ "type": "text", "text": "dynamic tool enabled" }],
                            "structuredContent": { "enabled": true },
                            "isError": false
                        }),
                    )
                }
            }
            _ => error_response(id, INVALID_PARAMS, "Tool not found"),
        }
    }

    fn echo(&self, id: Value, arguments: &Value) -> Value {
        if has_unknown_arguments(arguments, &["message"]) {
            return tool_error(id, "unknown echo argument");
        }
        let Some(message) = arguments.get("message").and_then(Value::as_str) else {
            return tool_error(id, "message must be a string");
        };
        if message.chars().count() > MAX_ECHO_CHARS {
            return tool_error(id, "message exceeds the fixture limit");
        }

        success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": message }],
                "structuredContent": { "message": message },
                "isError": false
            }),
        )
    }

    fn mutate(&self, id: Value, arguments: &Value) -> Value {
        if has_unknown_arguments(arguments, &["label", "changeTools"]) {
            return tool_error(id, "unknown mutation argument");
        }
        let label = match arguments.get("label") {
            Some(value) => match value.as_str() {
                Some(label) => label,
                None => return tool_error(id, "label must be a string"),
            },
            None => "mutation",
        };
        if label.chars().count() > 128 {
            return tool_error(id, "label exceeds the fixture limit");
        }

        let changed_tools = match arguments.get("changeTools") {
            Some(value) => match value.as_bool() {
                Some(changed_tools) => changed_tools,
                None => return tool_error(id, "changeTools must be a boolean"),
            },
            None => false,
        };
        let count = self.inner.mutation_count.fetch_add(1, Ordering::SeqCst) + 1;
        if changed_tools {
            self.inner
                .dynamic_tool_enabled
                .fetch_xor(true, Ordering::SeqCst);
            let _ = self.inner.notifications.send(json!({
                "jsonrpc": JSON_RPC_VERSION,
                "method": "notifications/tools/list_changed"
            }));
        }

        success_response(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": format!("mutation {count}: {label}")
                }],
                "structuredContent": {
                    "count": count,
                    "label": label,
                    "toolListChanged": changed_tools
                },
                "isError": false
            }),
        )
    }

    async fn slow(&self, id: Value, arguments: &Value) -> Value {
        if has_unknown_arguments(arguments, &["delayMs"]) {
            return tool_error(id, "unknown slow-tool argument");
        }
        let delay_ms = match arguments.get("delayMs") {
            Some(value) => match value.as_u64() {
                Some(delay_ms) => delay_ms,
                None => return tool_error(id, "delayMs must be a non-negative integer"),
            },
            None => 250,
        };
        if delay_ms > MAX_SLOW_DELAY_MS {
            return tool_error(id, "delayMs exceeds the fixture limit");
        }

        let key = request_key(&id).expect("request ID was validated");
        let cancellation = Arc::new(Notify::new());
        self.inner
            .in_flight
            .lock()
            .await
            .insert(key.clone(), cancellation.clone());

        let cancelled = tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => false,
            _ = cancellation.notified() => true,
        };
        self.inner.in_flight.lock().await.remove(&key);

        if cancelled {
            error_response(id, REQUEST_CANCELLED, "Request cancelled")
        } else {
            success_response(
                id,
                json!({
                    "content": [{
                        "type": "text",
                        "text": format!("completed after {delay_ms}ms")
                    }],
                    "structuredContent": { "delayMs": delay_ms },
                    "isError": false
                }),
            )
        }
    }

    fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            tool(
                "fixture.echo",
                "Echo a bounded string without modifying state.",
                json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "maxLength": MAX_ECHO_CHARS }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }),
                json!({
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false
                }),
            ),
            tool(
                "fixture.fail",
                "Return a deterministic MCP tool execution error.",
                empty_object_schema(),
                read_only_annotations(),
            ),
            tool(
                "fixture.malformed_result",
                "Return an intentionally malformed tool result for negative tests.",
                empty_object_schema(),
                read_only_annotations(),
            ),
            tool(
                "fixture.mutate",
                "Increment fixture state. Clients must require user approval.",
                json!({
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "maxLength": 128 },
                        "changeTools": { "type": "boolean" }
                    },
                    "additionalProperties": false
                }),
                json!({
                    "readOnlyHint": false,
                    "destructiveHint": false,
                    "idempotentHint": false,
                    "openWorldHint": false
                }),
            ),
            tool(
                "fixture.slow",
                "Complete after a bounded delay and honor MCP cancellation.",
                json!({
                    "type": "object",
                    "properties": {
                        "delayMs": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": MAX_SLOW_DELAY_MS
                        }
                    },
                    "additionalProperties": false
                }),
                read_only_annotations(),
            ),
        ];

        if self.inner.dynamic_tool_enabled.load(Ordering::SeqCst) {
            tools.push(tool(
                "fixture.dynamic",
                "A deterministic tool exposed after a list-change mutation.",
                empty_object_schema(),
                read_only_annotations(),
            ));
        }
        tools
    }
}

fn initialize_result(params: &Value) -> Option<Value> {
    let requested = params.get("protocolVersion")?.as_str()?;
    if !params.get("capabilities").is_some_and(Value::is_object) {
        return None;
    }
    let client_info = params.get("clientInfo")?;
    if !client_info.is_object()
        || client_info.get("name").and_then(Value::as_str).is_none()
        || client_info.get("version").and_then(Value::as_str).is_none()
    {
        return None;
    }
    let negotiated = SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .find(|version| **version == requested)
        .copied()
        .unwrap_or(LATEST_PROTOCOL_VERSION);

    Some(json!({
        "protocolVersion": negotiated,
        "capabilities": {
            "tools": { "listChanged": true }
        },
        "serverInfo": {
            "name": "dcc-mcp-fixture",
            "title": "DCC Offline MCP Fixture",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Deterministic offline fixture for DCC conformance tests."
    }))
}

fn tool(name: &str, description: &str, input_schema: Value, annotations: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "annotations": annotations
    })
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn read_only_annotations() -> Value {
    json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false
    })
}

fn request_key(id: &Value) -> Option<RequestKey> {
    match id {
        Value::Number(number) => Some(RequestKey::Number(number.to_string())),
        Value::String(value) => Some(RequestKey::String(value.clone())),
        _ => None,
    }
}

fn has_unknown_arguments(arguments: &Value, allowed: &[&str]) -> bool {
    arguments
        .as_object()
        .is_some_and(|arguments| arguments.keys().any(|key| !allowed.contains(&key.as_str())))
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": JSON_RPC_VERSION,
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": JSON_RPC_VERSION,
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn tool_error(id: Value, message: &str) -> Value {
    success_response(
        id,
        json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    )
}

#[cfg(test)]
mod tests {
    use dcc_core::domain::mcp_conformance::{
        MCP_CONFORMANCE_ECHO_TOOL, MCP_CONFORMANCE_MUTATING_TOOL,
    };

    use super::*;

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
    }

    #[tokio::test]
    async fn initialization_negotiates_supported_versions_and_declares_tools() {
        let server = FixtureServer::new();
        let response = server
            .handle_message(request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test", "version": "1" }
                }),
            ))
            .await
            .expect("initialize response");

        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(
            response["result"]["capabilities"]["tools"]["listChanged"],
            true
        );
    }

    #[tokio::test]
    async fn invalid_initialize_and_tool_arguments_fail_without_mutation() {
        let server = FixtureServer::new();
        let initialize = server
            .handle_message(request(1, "initialize", json!({})))
            .await
            .expect("initialize error");
        assert_eq!(initialize["error"]["code"], INVALID_PARAMS);

        let mutation = server
            .handle_message(request(
                2,
                "tools/call",
                json!({
                    "name": "fixture.mutate",
                    "arguments": { "changeTools": "yes" }
                }),
            ))
            .await
            .expect("mutation error");
        assert_eq!(mutation["result"]["isError"], true);
        assert_eq!(server.inner.mutation_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn tool_list_is_deterministic_and_marks_mutation_as_non_read_only() {
        let server = FixtureServer::new();
        let response = server
            .handle_message(request(1, "tools/list", json!({})))
            .await
            .expect("tools response");
        let tools = response["result"]["tools"].as_array().expect("tools array");
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "fixture.echo",
                "fixture.fail",
                "fixture.malformed_result",
                "fixture.mutate",
                "fixture.slow"
            ]
        );
        let mutation = tools
            .iter()
            .find(|tool| tool["name"] == "fixture.mutate")
            .expect("mutation tool");
        assert_eq!(mutation["annotations"]["readOnlyHint"], false);
        assert_eq!(mutation["annotations"]["openWorldHint"], false);
    }

    #[tokio::test]
    async fn conformance_contract_names_are_present_in_the_offline_fixture() {
        let response = FixtureServer::new()
            .handle_message(request(1, "tools/list", json!({})))
            .await
            .expect("tools response");
        let names = response["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&MCP_CONFORMANCE_ECHO_TOOL));
        assert!(names.contains(&MCP_CONFORMANCE_MUTATING_TOOL));
    }

    #[tokio::test]
    async fn echo_and_failure_results_follow_mcp_tool_error_semantics() {
        let server = FixtureServer::new();
        let echo = server
            .handle_message(request(
                1,
                "tools/call",
                json!({
                    "name": "fixture.echo",
                    "arguments": { "message": "hello" }
                }),
            ))
            .await
            .expect("echo response");
        assert_eq!(echo["result"]["structuredContent"]["message"], "hello");
        assert_eq!(echo["result"]["isError"], false);

        let failure = server
            .handle_message(request(
                2,
                "tools/call",
                json!({ "name": "fixture.fail", "arguments": {} }),
            ))
            .await
            .expect("failure response");
        assert_eq!(failure["result"]["isError"], true);
        assert!(failure.get("error").is_none());
    }

    #[tokio::test]
    async fn malformed_result_is_valid_json_rpc_but_invalid_mcp_content() {
        let server = FixtureServer::new();
        let response = server
            .handle_message(request(
                1,
                "tools/call",
                json!({ "name": "fixture.malformed_result", "arguments": {} }),
            ))
            .await
            .expect("malformed response");

        assert!(response["result"]["content"].is_string());
    }

    #[tokio::test]
    async fn mutation_can_toggle_a_tool_and_emit_list_changed() {
        let server = FixtureServer::new();
        let mut notifications = server.subscribe();
        let response = server
            .handle_message(request(
                1,
                "tools/call",
                json!({
                    "name": "fixture.mutate",
                    "arguments": { "label": "enable", "changeTools": true }
                }),
            ))
            .await
            .expect("mutation response");

        assert_eq!(response["result"]["structuredContent"]["count"], 1);
        let notification = notifications.recv().await.expect("list notification");
        assert_eq!(notification["method"], "notifications/tools/list_changed");

        let tools = server
            .handle_message(request(2, "tools/list", json!({})))
            .await
            .expect("tools response");
        assert!(tools["result"]["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .any(|tool| tool["name"] == "fixture.dynamic"));
    }

    #[tokio::test]
    async fn slow_tool_is_bounded_and_honors_cancellation() {
        let server = FixtureServer::new();
        let slow_server = server.clone();
        let slow = tokio::spawn(async move {
            slow_server
                .handle_message(request(
                    42,
                    "tools/call",
                    json!({
                        "name": "fixture.slow",
                        "arguments": { "delayMs": MAX_SLOW_DELAY_MS }
                    }),
                ))
                .await
                .expect("slow response")
        });

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if server
                    .inner
                    .in_flight
                    .lock()
                    .await
                    .contains_key(&RequestKey::Number("42".to_string()))
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("slow request registration");
        assert!(server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": { "requestId": 42, "reason": "test" }
            }))
            .await
            .is_none());

        let response = slow.await.expect("slow task");
        assert_eq!(response["error"]["code"], REQUEST_CANCELLED);

        let over_limit = server
            .handle_message(request(
                43,
                "tools/call",
                json!({
                    "name": "fixture.slow",
                    "arguments": { "delayMs": MAX_SLOW_DELAY_MS + 1 }
                }),
            ))
            .await
            .expect("bounded response");
        assert_eq!(over_limit["result"]["isError"], true);
    }
}
