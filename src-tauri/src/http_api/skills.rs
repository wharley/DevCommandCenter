//! The skills dialog's writes from the native app: save, delete, import a
//! previewed GitHub skill and compile the source into each agent's format,
//! all in the workspace's mutation queue like the desktop's own commands
//! (so they never land in the middle of an agent's edit).
use super::*;
use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::skills::{self, SkillImportPreview, SkillRecord};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaveInput {
    project_root: String,
    workspace_id: String,
    skill: SkillRecord,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteInput {
    project_root: String,
    workspace_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportInput {
    project_root: String,
    workspace_id: String,
    preview: SkillImportPreview,
    skill: SkillRecord,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompileInput {
    checkout_root: String,
    workspace_id: String,
}

/// Runs `write` on the authorised checkout in the workspace's queue.
async fn mutate(
    backend: &HttpBackend,
    workspace_id: String,
    root: &str,
    write: impl FnOnce(&str) -> Result<(), String> + Send + 'static,
) -> Result<Json<Value>, HttpApiError> {
    let done = backend
        .workspace
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| "workspace path encoding is unsupported".to_string())?;
                write(authorized)
            },
        )
        .await
        .map_err(HttpApiError::bad_request)?;
    done.map_err(HttpApiError::bad_request)?;
    Ok(Json(json!({ "ok": true })))
}

pub(super) async fn save(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<SaveInput>,
) -> Result<Json<Value>, HttpApiError> {
    let skill = input.skill;
    mutate(
        &backend,
        input.workspace_id,
        &input.project_root,
        move |root| skills::save_skill(root, skill),
    )
    .await
}

pub(super) async fn delete(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<DeleteInput>,
) -> Result<Json<Value>, HttpApiError> {
    let name = input.name;
    mutate(
        &backend,
        input.workspace_id,
        &input.project_root,
        move |root| skills::delete_skill(root, &name),
    )
    .await
}

pub(super) async fn import(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<ImportInput>,
) -> Result<Json<Value>, HttpApiError> {
    let (preview, skill) = (input.preview, input.skill);
    mutate(
        &backend,
        input.workspace_id,
        &input.project_root,
        move |root| skills::import_skill(root, preview, skill),
    )
    .await
}

pub(super) async fn compile(
    State(backend): State<Arc<HttpBackend>>,
    Json(input): Json<CompileInput>,
) -> Result<Json<Value>, HttpApiError> {
    mutate(
        &backend,
        input.workspace_id,
        &input.checkout_root,
        move |root| skills::compile_skills(root, root),
    )
    .await
}
