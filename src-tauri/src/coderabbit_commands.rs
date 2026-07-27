use tauri::{AppHandle, State};

use dcc_tauri::commands::coderabbit::CodeRabbitReviewJobsState;
use dcc_tauri::{
    commands::coderabbit::{
        CodeRabbitDiffFingerprint, WorkspaceCodeRabbitCliStatusInput,
        WorkspaceCodeRabbitCliStatusOutput, WorkspaceCodeRabbitDoctorInput,
        WorkspaceCodeRabbitDoctorOutput, WorkspaceCodeRabbitFingerprintInput,
        WorkspaceCodeRabbitLogoutInput, WorkspaceCodeRabbitLogoutOutput,
        WorkspaceCodeRabbitReviewHistoryInput, WorkspaceCodeRabbitReviewHistoryOutput,
        WorkspaceCodeRabbitReviewInput, WorkspaceCodeRabbitReviewJobInput,
        WorkspaceCodeRabbitReviewJobSnapshot, WorkspaceCodeRabbitReviewOutput,
        WorkspaceCodeRabbitReviewStartOutput, WorkspaceCodeRabbitSaveReviewInput,
        WorkspaceCodeRabbitStoredReviewInput, WorkspaceCodeRabbitStoredReviewOutput,
    },
    state::WorkspaceCommandState,
};

#[tauri::command]
pub async fn workspace_coderabbit_cli_status(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitCliStatusInput,
) -> Result<WorkspaceCodeRabbitCliStatusOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_cli_status(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_logout(
    input: WorkspaceCodeRabbitLogoutInput,
) -> Result<WorkspaceCodeRabbitLogoutOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_logout(input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_doctor(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitDoctorInput,
) -> Result<WorkspaceCodeRabbitDoctorOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_doctor(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_diff_fingerprint(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitFingerprintInput,
) -> Result<CodeRabbitDiffFingerprint, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_diff_fingerprint(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitReviewInput,
) -> Result<WorkspaceCodeRabbitReviewOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_start(
    app: AppHandle,
    workspace_state: State<'_, WorkspaceCommandState>,
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewInput,
) -> Result<WorkspaceCodeRabbitReviewStartOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_start(
        app,
        workspace_state,
        jobs_state,
        input,
    )
    .await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_job(
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewJobInput,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_job(jobs_state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_cancel(
    jobs_state: State<'_, CodeRabbitReviewJobsState>,
    input: WorkspaceCodeRabbitReviewJobInput,
) -> Result<WorkspaceCodeRabbitReviewJobSnapshot, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_cancel(jobs_state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_load(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitStoredReviewInput,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_load(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_save(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitSaveReviewInput,
) -> Result<WorkspaceCodeRabbitStoredReviewOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_save(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_history(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitReviewHistoryInput,
) -> Result<WorkspaceCodeRabbitReviewHistoryOutput, String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_history(state, input).await
}

#[tauri::command]
pub async fn workspace_coderabbit_review_clear(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCodeRabbitStoredReviewInput,
) -> Result<(), String> {
    dcc_tauri::commands::coderabbit::workspace_coderabbit_review_clear(state, input).await
}
