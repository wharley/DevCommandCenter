//! An agent's sources from its page and the room (`agent_source`): create
//! one the agent proposed, switch it, delete it, and read it now (the app's
//! clock, once it is due); and the "Nova fonte" form: the connectors the
//! agent may read, a connector's read-only tools, a test read, and the
//! source the person makes. Local only: the relay never forwards
//! `/api/v1/agents/`.
use super::*;

#[derive(Deserialize)]
pub(super) struct SwitchInput {
    enabled: bool,
}

fn found(changed: bool) -> Result<Json<Value>, HttpApiError> {
    if changed {
        Ok(Json(json!({ "ok": true })))
    } else {
        Err(HttpApiError::not_found("source not found"))
    }
}

pub(super) async fn switch(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, source_id)): Path<(String, String)>,
    Json(input): Json<SwitchInput>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .set_agent_source(agent_id.trim(), source_id.trim(), input.enabled)
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn delete(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, source_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let changed = backend
        .sessions
        .delete_agent_source(agent_id.trim(), source_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    found(changed)
}

pub(super) async fn read(
    State(backend): State<Arc<HttpBackend>>,
    Path((_agent_id, source_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    let gateway = backend
        .app
        .get()
        .and_then(|app| app.connectors())
        .map(|(_, gateway)| gateway)
        .ok_or_else(|| HttpApiError::not_found("connectors are not available"))?;
    let read = dcc_tauri::source_read::read_source(&backend.sessions, &gateway, source_id.trim())
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(read)))
}

fn gateway(
    backend: &HttpBackend,
) -> Result<dcc_tauri::connector_gateway::ConnectorGateway, HttpApiError> {
    backend
        .app
        .get()
        .and_then(|app| app.connectors())
        .map(|(_, gateway)| gateway)
        .ok_or_else(|| HttpApiError::not_found("connectors are not available"))
}

/// The connectors the agent may read, for the form's first choice.
pub(super) async fn form_connectors(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    let connectors: Vec<Value> = backend
        .sessions
        .source_form_connectors(agent_id.trim())
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?
        .into_iter()
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect();
    Ok(Json(json!({ "connectors": connectors })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolsInput {
    connector_id: String,
}

/// The connector's read-only tools, asked of it now.
pub(super) async fn form_tools(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<ToolsInput>,
) -> Result<Json<Value>, HttpApiError> {
    let gateway = gateway(&backend)?;
    let tools = dcc_tauri::source_form::tools(
        &backend.sessions,
        &gateway,
        agent_id.trim(),
        input.connector_id.trim(),
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "tools": tools })))
}

/// "Testar leitura": one read with what the form holds, nothing stored.
pub(super) async fn form_test(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<dcc_tauri::state::SourceInput>,
) -> Result<Json<Value>, HttpApiError> {
    let gateway = gateway(&backend)?;
    let read = dcc_tauri::source_form::test(&backend.sessions, &gateway, agent_id.trim(), input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(read)))
}

/// "Criar fonte": stored and on.
pub(super) async fn create(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<dcc_tauri::state::SourceInput>,
) -> Result<Json<Value>, HttpApiError> {
    let source = backend
        .sessions
        .create_agent_source(agent_id.trim(), input)
        .await
        .map_err(|error| HttpApiError::bad_request(error.to_string()))?;
    Ok(Json(json!(source)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SuggestInput {
    connector_id: String,
    watch: String,
}

/// The fill button: the agent picks the tool and its arguments for what
/// the person wrote (one model run, only when clicked).
pub(super) async fn form_suggest(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<SuggestInput>,
) -> Result<Json<Value>, HttpApiError> {
    let gateway = gateway(&backend)?;
    let pick = dcc_tauri::source_form::suggest(
        &backend.sessions,
        &gateway,
        agent_id.trim(),
        input.connector_id.trim(),
        &input.watch,
    )
    .await
    .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(pick)))
}
