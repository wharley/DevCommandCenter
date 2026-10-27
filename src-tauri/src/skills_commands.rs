//! Provider-neutral skills (`dcc_infra::skills`) as Tauri commands. The
//! writes run in the workspace's mutation queue so they never land in the
//! middle of an agent's edit; the native app reaches the same ones over the
//! loopback API (`/api/v1/skills/*`).

use tauri::{command, State};

use crate::{db_error, ApiResult};
use dcc_core::domain::workspace::WorkspaceId;
use dcc_infra::skills::{self, SkillImportPreview, SkillRecord};
use dcc_infra::skills_context::detect_context_sources;
pub use dcc_infra::skills_context::SkillContextDetection;
use dcc_tauri::state::WorkspaceCommandState;

#[command]
pub async fn skills_import_preview(source_url: String) -> ApiResult<SkillImportPreview> {
    tauri::async_runtime::spawn_blocking(move || skills::preview(&source_url))
        .await
        .map_err(|error| db_error(error.to_string()))?
        .map_err(db_error)
}

#[command]
pub async fn skills_import(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    preview: SkillImportPreview,
    skill: SkillRecord,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                skills::import_skill(authorized, preview, skill).map_err(db_error)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub fn skills_list(project_root: String) -> ApiResult<Vec<SkillRecord>> {
    Ok(skills::load_skills(&project_root))
}

#[command]
pub async fn skills_save(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    skill: SkillRecord,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                skills::save_skill(authorized, skill).map_err(db_error)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub async fn skills_delete(
    state: State<'_, WorkspaceCommandState>,
    project_root: String,
    workspace_id: String,
    name: String,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &project_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                skills::delete_skill(authorized, &name).map_err(db_error)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub async fn skills_compile(
    state: State<'_, WorkspaceCommandState>,
    checkout_root: String,
    workspace_id: String,
) -> ApiResult<()> {
    match state
        .run_registered_workspace_mutation_for_workspace_id(
            &WorkspaceId(workspace_id),
            &checkout_root,
            move |authorized| {
                let authorized = authorized
                    .to_str()
                    .ok_or_else(|| db_error("workspace path encoding is unsupported"))?;
                skills::compile_skills(authorized, authorized).map_err(db_error)
            },
        )
        .await
    {
        Ok(result) => result,
        Err(error) => Err(db_error(error)),
    }
}

#[command]
pub fn skills_detect_context(
    project_root: String,
    target_root: Option<String>,
) -> ApiResult<Vec<SkillContextDetection>> {
    Ok(detect_context_sources(
        &project_root,
        target_root.as_deref(),
    ))
}
