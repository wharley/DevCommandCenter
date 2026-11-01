//! The branch a local-direct task starts on, for the native app: the
//! branches the checkout can switch to, and the switch itself. Both run the
//! desktop's own commands, so a started conversation or a running agent
//! still refuses the switch.
use super::*;
use dcc_tauri::commands::local_branches::{
    self, LocalBranchesInput, LocalBranchesOutput, SwitchLocalBranchInput,
};

fn answer(result: Result<LocalBranchesOutput, String>) -> Result<Json<Value>, HttpApiError> {
    let output = result.map_err(HttpApiError::bad_request)?;
    serde_json::to_value(output)
        .map(Json)
        .map_err(|error| HttpApiError::internal(error.to_string()))
}

pub(super) async fn list(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<LocalBranchesInput>,
) -> Result<Json<Value>, HttpApiError> {
    answer(local_branches::workspace_local_branches_with_state(&backend.workspace, input).await)
}

pub(super) async fn switch(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SwitchLocalBranchInput>,
) -> Result<Json<Value>, HttpApiError> {
    answer(
        local_branches::workspace_switch_local_branch_with_state(&backend.workspace, input).await,
    )
}
