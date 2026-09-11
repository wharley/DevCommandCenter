use dcc_tauri::{
    commands::feedback::{self, FeedbackContext, FeedbackInput, FeedbackIssue, FeedbackPage},
    state::WorkspaceCommandState,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn dcc_feedback_context(
    app: AppHandle,
    state: State<'_, WorkspaceCommandState>,
) -> Result<FeedbackContext, String> {
    let state = state.inner().clone();
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || feedback::context(&state, version))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn dcc_feedback_list(
    state: State<'_, WorkspaceCommandState>,
    login: String,
    page: u32,
) -> Result<FeedbackPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let current = feedback::context(&state, String::new())?;
        if login != current.login {
            return Err("FEEDBACK_ACCOUNT_CHANGED".into());
        }
        feedback::list(&login, page)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn dcc_feedback_create(
    app: AppHandle,
    state: State<'_, WorkspaceCommandState>,
    input: FeedbackInput,
) -> Result<FeedbackIssue, String> {
    let state = state.inner().clone();
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let context = feedback::context(&state, version)?;
        feedback::create(&state.db_path, input, context)
    })
    .await
    .map_err(|e| e.to_string())?
}
