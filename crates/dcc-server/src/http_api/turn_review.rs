//! The last turn's review for the native app's changes list: what the turn
//! changed (and one file's captured patch), and Guarded Undo in its two
//! steps — prepare (read-only, a preview with a token) and execute (with
//! that token, once the person confirmed). "Editar a partir daqui" plans
//! and carries out a rewind the same way, restoring files through Guarded
//! Undo. They run the desktop's own commands, which hold the capture and the
//! undo journal.
use super::*;
use dcc_tauri::commands::session_commands::{
    self, ExecuteConversationRewindInput, ExecuteGuardedUndoInput, LastTurnReviewInput,
    PrepareConversationRewindInput, PrepareGuardedUndoInput, TurnReviewFileDiffInput,
};

fn reply<T: Serialize>(result: Result<T, String>) -> Result<Json<Value>, HttpApiError> {
    let value = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LastTurnInput {
    workspace_id: String,
    #[serde(default)]
    turn_id: Option<String>,
}

pub(super) async fn last_turn(
    State(backend): State<Arc<HttpBackend>>,
    Path(session_id): Path<String>,
    Json(input): Json<LastTurnInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(
        session_commands::last_turn_review_with_state(
            &backend.sessions,
            LastTurnReviewInput {
                session_id,
                workspace_id: input.workspace_id,
                turn_id: input.turn_id,
            },
        )
        .await,
    )
}

pub(super) async fn file_diff(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<TurnReviewFileDiffInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(session_commands::turn_review_file_diff_with_state(&backend.sessions, input).await)
}

pub(super) async fn prepare_undo(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PrepareGuardedUndoInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(session_commands::prepare_guarded_undo_with_state(&backend.sessions, input).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExecuteInput {
    preview_token: String,
}

/// Only a token from a preparation runs, and only the files it previewed.
pub(super) async fn execute_undo(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ExecuteInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(
        session_commands::execute_guarded_undo_with_state(
            &backend.sessions,
            ExecuteGuardedUndoInput {
                preview_token: input.preview_token,
                confirmed: true,
            },
        )
        .await,
    )
}

/// Read-only: what "Editar a partir daqui" would remove and restore.
pub(super) async fn prepare_rewind(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<PrepareConversationRewindInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(session_commands::prepare_conversation_rewind_with_state(&backend.sessions, input).await)
}

/// The prepared rewind; a conversation that moved since refuses it.
pub(super) async fn execute_rewind(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ExecuteConversationRewindInput>,
) -> Result<Json<Value>, HttpApiError> {
    reply(session_commands::execute_conversation_rewind_with_state(&backend.sessions, input).await)
}
