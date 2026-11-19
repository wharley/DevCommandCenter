//! "Edit from here" over HTTP: what undoing a turn and everything after it
//! would do, then doing it. The same commands the app's timeline calls;
//! the files are restored first and only when the caller asks.
use super::*;
use dcc_tauri::commands::session_commands::{
    execute_conversation_rewind_with_state, prepare_conversation_rewind_with_state,
    ExecuteConversationRewindInput, PrepareConversationRewindInput,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PrepareBody {
    anchor_turn_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExecuteBody {
    anchor_turn_id: String,
    removed_turn_ids: Vec<String>,
    #[serde(default)]
    restore_files: bool,
    #[serde(default)]
    continue_in_new_thread: bool,
}

pub(super) async fn prepare(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<PrepareBody>,
) -> Result<Json<Value>, HttpApiError> {
    let input = PrepareConversationRewindInput {
        session_id,
        anchor_turn_id: body.anchor_turn_id,
    };
    let output = prepare_conversation_rewind_with_state(&backend.sessions, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(output)))
}

pub(super) async fn execute(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(body): Json<ExecuteBody>,
) -> Result<Json<Value>, HttpApiError> {
    let input = ExecuteConversationRewindInput {
        session_id,
        anchor_turn_id: body.anchor_turn_id,
        removed_turn_ids: body.removed_turn_ids,
        restore_files: body.restore_files,
        continue_in_new_thread: body.continue_in_new_thread,
    };
    let output = execute_conversation_rewind_with_state(&backend.sessions, input)
        .await
        .map_err(HttpApiError::bad_request)?;
    Ok(Json(json!(output)))
}
