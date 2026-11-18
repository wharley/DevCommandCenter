//! One request to a connector, for a client of the engine (an app, a
//! process beside it): `tools/list` to read its tools and their schemas, or
//! `tools/call` to run one, through the same gateway the agents use, with
//! the connector's own sign-in. The engine's key already says the caller is
//! the person's own; what the call may do is what the connector's sign-in
//! allows.

use super::*;
use dcc_core::domain::agent_access::AccessLevel;
use dcc_core::domain::mcp::McpDefinitionId;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConnectorRequest {
    definition_id: String,
    /// `tools/list` or `tools/call`.
    method: String,
    #[serde(default)]
    params: Value,
}

pub(super) async fn request(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ConnectorRequest>,
) -> Result<Json<Value>, HttpApiError> {
    if !matches!(input.method.as_str(), "tools/list" | "tools/call") {
        return Err(HttpApiError::bad_request(
            "only tools/list and tools/call".to_string(),
        ));
    }
    let gateway = backend
        .app
        .get()
        .and_then(|app| app.connectors())
        .map(|(_, gateway)| gateway)
        .ok_or_else(|| HttpApiError::not_found("connectors are not available"))?;
    let params = if input.params.is_null() {
        json!({})
    } else {
        input.params
    };
    dcc_tauri::source_read::request_once(
        &gateway,
        &McpDefinitionId(input.definition_id),
        &input.method,
        params,
        AccessLevel::Write,
    )
    .await
    .map(Json)
    .map_err(HttpApiError::bad_request)
}
