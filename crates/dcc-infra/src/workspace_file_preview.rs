//! The two sides of a changed file, read from git and the worktree: the
//! original (HEAD, or the base branch) and the modified text the diff shows.
//! Read-only; shared by the desktop's preview command and the native app.

use std::fs;
use std::path::{Component, Path, PathBuf};

use dcc_core::domain::workspace_git::WorkspaceGitPreviewScope;

use crate::git::{git_output_err, run_git_output};
use crate::workspace_git::normalize_git_relative_path;

pub fn is_path_inside(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

pub fn validate_git_relative_path(path: &str) -> Result<String, String> {
    let p = normalize_git_relative_path(path);
    if p.is_empty() {
        return Err("path is empty".to_string());
    }
    if p.contains('\0') {
        return Err("invalid path".to_string());
    }
    let path = Path::new(&p);
    if path.is_absolute() {
        return Err("invalid path".to_string());
    }
    let has_normal_component = path.components().any(|component| match component {
        Component::Normal(_) => true,
        Component::CurDir => false,
        Component::ParentDir | Component::RootDir | Component::Prefix(_) => false,
    });
    if !has_normal_component {
        return Err("invalid path".to_string());
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("invalid path".to_string());
    }
    Ok(p)
}

pub fn run_git_show_text(
    root: &str,
    revision_path: &str,
    command: &str,
) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["show", revision_path])?;
    if output.status.success() {
        return Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let missing_markers = [
        "Path '",
        "path '",
        "does not exist",
        "exists on disk, but not in",
        "does not have a commit",
    ];
    if missing_markers.iter().any(|marker| stderr.contains(marker)) {
        return Ok(None);
    }

    Err(git_output_err(command, &output.stderr))
}

pub fn resolve_worktree_file_path(root: &str, rel: &str) -> Result<Option<PathBuf>, String> {
    let root_canonical = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let candidate = root_canonical.join(rel);
    let metadata = match fs::symlink_metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() && !metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !is_symlink_target_allowed(root, &canonical, &root_canonical) {
        return Err("path escapes workspace".to_string());
    }
    if !canonical.is_file() {
        return Ok(None);
    }
    Ok(Some(canonical))
}

/// A path that resolves inside the worktree, or inside the repository's main
/// checkout: a worktree often links `.env` back to main (`ln -sf`), and that
/// file belongs to the same repository. Links anywhere else stay refused, so
/// a cloned repo cannot point the editor at `~/.ssh`.
pub fn is_symlink_target_allowed(root: &str, canonical: &Path, root_canonical: &Path) -> bool {
    if is_path_inside(canonical, root_canonical) {
        return true;
    }
    main_checkout_root(root).is_some_and(|main| is_path_inside(canonical, &main))
}

/// The main checkout of the repository `root` belongs to: the folder holding
/// the shared `.git`. `None` for a bare repository or when Git cannot tell.
pub fn main_checkout_root(root: &str) -> Option<PathBuf> {
    let output = run_git_output(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()
    .filter(|output| output.status.success())?;
    let common_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    if common_dir.file_name()? != ".git" {
        return None;
    }
    common_dir.parent()?.canonicalize().ok()
}

pub fn read_worktree_file_text(root: &str, rel: &str) -> Result<Option<String>, String> {
    let Some(path) = resolve_worktree_file_path(root, rel)? else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

/// `workspaceGitFilePreviewContent`: (original, modified, whether the diff
/// reads as one block, a file added or deleted).
pub fn file_preview_content(
    root: &str,
    relative_path: &str,
    status: &str,
    scope: &WorkspaceGitPreviewScope,
    base_branch: Option<&str>,
) -> Result<(String, String, bool), String> {
    let root = root.trim();
    if root.is_empty() {
        return Err("workspace_root is empty".to_string());
    }
    let rel = validate_git_relative_path(relative_path)?;
    let absolute = PathBuf::from(root).join(&rel);
    let status = status.trim();
    let base_branch = base_branch.map(str::trim).filter(|value| !value.is_empty());
    let (original_text, modified_text, inline) = match scope {
        WorkspaceGitPreviewScope::Committed => {
            let base = base_branch
                .ok_or_else(|| "base_branch is required for committed previews".to_string())?;
            let original = run_git_show_text(root, &format!("{base}:{rel}"), "git show base")?
                .unwrap_or_default();
            let modified = run_git_show_text(root, &format!("HEAD:{rel}"), "git show HEAD")?
                .unwrap_or_default();
            (original, modified, matches!(status, "A" | "D" | "?"))
        }
        WorkspaceGitPreviewScope::Staged | WorkspaceGitPreviewScope::Unstaged => {
            let original = if status == "?" {
                String::new()
            } else {
                run_git_show_text(root, &format!("HEAD:{rel}"), "git show HEAD")?
                    .unwrap_or_default()
            };
            let modified = match status {
                "D" => String::new(),
                "?" => read_worktree_file_text(root, &rel)?.unwrap_or_default(),
                _ => read_worktree_file_text(root, &rel)?.unwrap_or_else(|| {
                    run_git_show_text(root, &format!(":{rel}"), "git show index")
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                }),
            };
            (original, modified, matches!(status, "A" | "D" | "?"))
        }
    };
    if original_text.is_empty() && modified_text.is_empty() {
        return Err("no content available for this file".to_string());
    }
    if !absolute.is_file() && *scope != WorkspaceGitPreviewScope::Committed && status != "D" {
        return Err("file not found".to_string());
    }
    Ok((original_text, modified_text, inline))
}
