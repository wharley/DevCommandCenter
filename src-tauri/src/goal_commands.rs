//! The provider's own goal (`/goal`): what only the app does with it. Setting
//! and clearing a goal are ordinary turns sent to the provider.
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use tauri::State;

/// "Fechar" on a finished goal.
#[tauri::command]
pub async fn session_dismiss_goal(
    state: State<'_, SessionCommandState>,
    session_id: String,
) -> Result<(), String> {
    state
        .dismiss_finished_goal(&SessionId(session_id))
        .await
        .map_err(|error| error.to_string())
}
