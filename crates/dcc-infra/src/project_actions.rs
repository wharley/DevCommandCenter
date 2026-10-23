//! Changing a project from the sidebar: its identity (name, icon, color,
//! logo) and the folders it links into new tasks. Shared by the Tauri
//! commands and the native app.

use std::path::Path;

use dcc_core::domain::repository::{Repository, RepositoryId};
use dcc_core::ports::RepositoryRepo;

use crate::db::SqliteWorkspaceRepo;

/// What the edit dialog sends; empty fields mean "automatic".
#[derive(Clone, Debug, Default)]
pub struct ProjectIdentity {
    pub display_name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// A small PNG data URI; `None` removes it.
    pub logo: Option<String>,
}

pub fn normalize_repository_display_name(
    display_name: Option<&str>,
    technical_name: &str,
) -> Option<String> {
    display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !value.eq_ignore_ascii_case(technical_name.trim()))
        .map(ToString::to_string)
}

// Empty means "auto": the desktop picks a Brazilian-fauna mascot from a hash of
// the project path. Every listed value, `folder`/`slate` included, is an explicit pick.
pub const PROJECT_ICONS: [&str; 23] = [
    "logo", "capivara", "tucano", "arara", "mico", "tatu", "onca", "jabuti", "boto", "sapo",
    "polvo", "folder", "terminal", "code", "layers", "package", "database", "globe", "rocket",
    "branch", "cpu", "shield", "wrench",
];
pub const PROJECT_COLORS: [&str; 12] = [
    "slate", "sky", "cyan", "emerald", "amber", "orange", "rose", "violet", "indigo", "fuchsia",
    "lime", "pink",
];
/// The desktop downsizes uploads to a 64px PNG, which is a few KB; the cap
/// keeps a hostile or broken client from bloating every project listing.
pub const MAX_PROJECT_LOGO_CHARS: usize = 96 * 1024;

pub fn normalize_repository_logo(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(payload) = value.strip_prefix("data:image/png;base64,") else {
        return Err("project logo must be a PNG data URI".to_string());
    };
    if value.len() > MAX_PROJECT_LOGO_CHARS {
        return Err("project logo is too large".to_string());
    }
    if payload.is_empty()
        || !payload
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    {
        return Err("project logo is not valid base64".to_string());
    }
    Ok(Some(value.to_string()))
}

pub fn normalize_repository_visual(
    value: Option<&str>,
    allowed: &[&str],
    field: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if allowed.contains(&value) {
        return Ok(Some(value.to_string()));
    }
    Err(format!("unsupported project {field}"))
}

/// The most folders one project brings into a task.
pub const MAX_LINKED_FOLDERS: usize = 8;

/// Checks the folders a project links: absolute folders that exist, distinct,
/// and neither the project itself nor nested in or around it. A Git checkout
/// joins a task as another project; any other folder joins as context.
pub fn normalize_linked_folders(
    project_root: &str,
    paths: &[String],
) -> Result<Vec<String>, String> {
    if paths.len() > MAX_LINKED_FOLDERS {
        return Err(format!(
            "a project links at most {MAX_LINKED_FOLDERS} folders"
        ));
    }
    let project_root = Path::new(project_root.trim().trim_end_matches('/'));
    let mut normalized: Vec<String> = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.trim().trim_end_matches('/');
        let folder = Path::new(path);
        if !folder.is_absolute() {
            return Err(format!("linked folder must be an absolute path: {path}"));
        }
        if !folder.is_dir() {
            return Err(format!("linked folder does not exist: {path}"));
        }
        if folder.starts_with(project_root) || project_root.starts_with(folder) {
            return Err(format!(
                "linked folder cannot be the project, inside it or around it: {path}"
            ));
        }
        if normalized
            .iter()
            .any(|other| Path::new(other).starts_with(folder) || folder.starts_with(other))
        {
            return Err(format!("linked folders cannot repeat or nest: {path}"));
        }
        normalized.push(path.to_string());
    }
    Ok(normalized)
}

pub async fn update_repository_identity(
    repo: &SqliteWorkspaceRepo,
    repository_id: &str,
    input: &ProjectIdentity,
) -> Result<Repository, String> {
    let repository_id = RepositoryId(repository_id.trim().to_string());
    let existing = repo
        .get_repository(&repository_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "project not found".to_string())?;
    let display_name =
        normalize_repository_display_name(input.display_name.as_deref(), &existing.name);
    let icon = normalize_repository_visual(input.icon.as_deref(), &PROJECT_ICONS, "icon")?;
    let color = normalize_repository_visual(input.color.as_deref(), &PROJECT_COLORS, "color")?;
    let logo = normalize_repository_logo(input.logo.as_deref())?;
    if icon.as_deref() == Some("logo") && logo.is_none() {
        return Err("choose a logo image before selecting it as the project icon".to_string());
    }
    let updated = repo
        .update_repository_identity(
            &repository_id,
            display_name.as_deref(),
            icon.as_deref(),
            color.as_deref(),
            logo.as_deref(),
        )
        .map_err(|error| error.to_string())?;
    if !updated {
        return Err("project not found".to_string());
    }
    repo.get_repository(&repository_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "project not found".to_string())
}

/// The linked folders without Git: they join a task as context only.
pub fn linked_context_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| !crate::git::is_git_repo(Path::new(path)))
        .cloned()
        .collect()
}

/// Saves the folders a project links, checked; returns them as stored.
pub async fn set_repository_linked_folders(
    repo: &SqliteWorkspaceRepo,
    repository_id: &str,
    paths: &[String],
) -> Result<Vec<String>, String> {
    let repository_id = RepositoryId(repository_id.trim().to_string());
    let repository = repo
        .get_repository(&repository_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "project not found".to_string())?;
    let paths = normalize_linked_folders(&repository.root_path, paths)?;
    repo.set_repository_linked_folders(&repository_id, &paths)
        .map_err(|error| error.to_string())?;
    Ok(paths)
}
