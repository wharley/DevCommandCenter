use tauri::State;

use dcc_tauri::{
    commands::coderabbit::{
        CodeRabbitDiffFingerprint, WorkspaceCodeRabbitCliStatusInput,
        WorkspaceCodeRabbitCliStatusOutput, WorkspaceCodeRabbitDoctorInput,
        WorkspaceCodeRabbitDoctorOutput, WorkspaceCodeRabbitFingerprintInput,
        WorkspaceCodeRabbitReviewInput, WorkspaceCodeRabbitReviewOutput,
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
