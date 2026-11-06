//! A resident agent's memory for its page: the notes, how full it is and
//! its versions; the person adds, edits and deletes notes and brings an
//! earlier version back. The agent saves its own notes with
//! `dcc_agent_remember` (`agent_memory_tools`).
use super::*;
use dcc_core::domain::agent_memory::usage_percent;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NoteInput {
    body: String,
}

fn bad_request(error: impl ToString) -> HttpApiError {
    HttpApiError::bad_request(error.to_string())
}

fn memory(backend: &HttpBackend, agent_id: &str) -> Result<Json<Value>, HttpApiError> {
    let notes = backend
        .sessions
        .agent_memory(agent_id)
        .map_err(bad_request)?;
    let versions = backend
        .sessions
        .agent_memory_versions(agent_id)
        .map_err(bad_request)?;
    Ok(Json(json!({
        "usage": usage_percent(&notes),
        "notes": notes,
        "versions": versions,
    })))
}

pub(super) async fn show(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, HttpApiError> {
    memory(&backend, agent_id.trim())
}

pub(super) async fn add(
    State(backend): State<Arc<HttpBackend>>,
    Path(agent_id): Path<String>,
    Json(input): Json<NoteInput>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .add_agent_memory_note(agent_id.trim(), &input.body)
        .map_err(bad_request)?;
    memory(&backend, agent_id.trim())
}

pub(super) async fn edit(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, note_id)): Path<(String, String)>,
    Json(input): Json<NoteInput>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .edit_agent_memory_note(agent_id.trim(), note_id.trim(), &input.body)
        .map_err(bad_request)?;
    memory(&backend, agent_id.trim())
}

pub(super) async fn remove(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, note_id)): Path<(String, String)>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .remove_agent_memory_note(agent_id.trim(), note_id.trim())
        .map_err(bad_request)?;
    memory(&backend, agent_id.trim())
}

pub(super) async fn restore(
    State(backend): State<Arc<HttpBackend>>,
    Path((agent_id, version_id)): Path<(String, i64)>,
) -> Result<Json<Value>, HttpApiError> {
    backend
        .sessions
        .restore_agent_memory_version(agent_id.trim(), version_id)
        .map_err(bad_request)?;
    memory(&backend, agent_id.trim())
}
