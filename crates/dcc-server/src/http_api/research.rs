//! The Researcher's ideas for the native app: a new idea starts with its
//! folder in `~/dcc-ideias`, its empty `IDEIA.md` and the mark that keeps
//! it out of the projects (`research_create_idea`); the app then opens its
//! task with the Researcher. An idea is then
//! published (a GitHub repository, the folder moved, a project) or
//! discarded (task, conversation, records and folder). Local only: the
//! relay never forwards `/api/v1/research/`.
use super::*;
use dcc_tauri::commands::research_commands::{self, IdeaRootInput, PublishIdeaInput};

pub(super) async fn create_idea(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let idea = research_commands::research_create_idea_with_state(&backend.workspace)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(idea)))
}

pub(super) async fn publish_idea(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PublishIdeaInput>,
) -> Result<Json<Value>, HttpApiError> {
    let published = research_commands::research_publish_idea_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(published)))
}

pub(super) async fn discard_idea(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<IdeaRootInput>,
) -> Result<Json<Value>, HttpApiError> {
    research_commands::research_discard_idea_with_state(&backend.workspace, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}
