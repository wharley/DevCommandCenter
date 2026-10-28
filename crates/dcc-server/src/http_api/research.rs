//! The Researcher's ideas for the native app: a new idea starts with its
//! folder in `~/dcc-ideias`, its empty `IDEIA.md` and the mark that keeps
//! it out of the projects (`research_create_idea`); the app then opens its
//! task with the Researcher. Local only: the relay never forwards
//! `/api/v1/research/`.
use super::*;

pub(super) async fn create_idea(
    State(backend): State<Arc<HttpBackend>>,
) -> Result<Json<Value>, HttpApiError> {
    let idea =
        dcc_tauri::commands::research_commands::research_create_idea_with_state(&backend.workspace)
            .await
            .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(idea)))
}
