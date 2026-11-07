//! A resident agent's memory for its page: the notes (the person's and each
//! project's), how full each layer is and its versions; the person adds,
//! edits and deletes notes and brings an earlier version back. The agent saves its own notes with
//! `dcc_agent_remember` (`agent_memory_tools`).
use super::*;
use dcc_core::domain::agent_memory::usage_percent;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NoteInput {
    body: String,
    /// Adding: the project the note holds in; none: the person's.
    #[serde(default)]
    project_id: Option<String>,
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
    let mut projects: Vec<&str> = notes
        .iter()
        .filter_map(|note| note.project_id.as_deref())
        .collect();
    projects.sort_unstable();
    projects.dedup();
    let project_usage: serde_json::Map<String, Value> = projects
        .into_iter()
        .map(|project| {
            (
                project.to_string(),
                json!(usage_percent(&notes, Some(project))),
            )
        })
        .collect();
    Ok(Json(json!({
        "usage": usage_percent(&notes, None),
        "projectUsage": project_usage,
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
        .add_agent_memory_note(
            agent_id.trim(),
            &input.body,
            input
                .project_id
                .as_deref()
                .map(str::trim)
                .filter(|project| !project.is_empty()),
        )
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
