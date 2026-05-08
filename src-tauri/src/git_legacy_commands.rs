use dcc_tauri::git::parse_git_status_porcelain_z;
use serde_json::Value;

use crate::{
    git_legacy::{
        build_review_diffs_for_path, git_current_branch_impl, git_local_branch_names,
        git_output_in_dir, run_git,
    },
    ApiResult,
};

#[tauri::command]
pub(crate) fn git_get_current_branch(project_path: String) -> ApiResult<String> {
    git_current_branch_impl(&project_path)
}

#[tauri::command]
pub(crate) fn git_get_local_branches(project_path: String) -> ApiResult<Value> {
    let branches = git_local_branch_names(&project_path)?;
    Ok(Value::Array(
        branches.into_iter().map(Value::String).collect(),
    ))
}

#[tauri::command]
pub(crate) fn git_get_status(project_path: String) -> ApiResult<Value> {
    let is_repo = std::path::Path::new(&project_path).join(".git").exists();

    if !is_repo {
        return Ok(serde_json::json!({
            "isRepo": false,
            "isDirty": false,
            "staged": [],
            "unstaged": [],
            "untracked": []
        }));
    }

    let status_output = match git_output_in_dir(&project_path, &["status", "--porcelain", "-z"]) {
        Ok(output) => output,
        Err(_) => {
            return Ok(serde_json::json!({
                "isRepo": true,
                "isDirty": false,
                "staged": [],
                "unstaged": [],
                "untracked": []
            }));
        }
    };
    if !status_output.status.success() {
        return Ok(serde_json::json!({
            "isRepo": true,
            "isDirty": false,
            "staged": [],
            "unstaged": [],
            "untracked": []
        }));
    }

    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();

    for entry in parse_git_status_porcelain_z(&status_output.stdout) {
        if entry.index_status == '?' && entry.worktree_status == '?' {
            untracked.push(entry.path);
            continue;
        }

        if entry.index_status != ' ' && entry.index_status != '?' {
            staged.push(entry.path.clone());
        }

        if entry.worktree_status != ' ' && entry.worktree_status != '?' {
            if !staged.contains(&entry.path) {
                unstaged.push(entry.path);
            }
        }
    }

    let is_dirty = !staged.is_empty() || !unstaged.is_empty() || !untracked.is_empty();

    Ok(serde_json::json!({
        "isRepo": true,
        "isDirty": is_dirty,
        "staged": staged,
        "unstaged": unstaged,
        "untracked": untracked
    }))
}

#[tauri::command]
pub(crate) fn git_commit(
    project_path: String,
    message: String,
    _files: Option<Vec<String>>,
) -> ApiResult<Value> {
    if let Err(e) = run_git(&project_path, &["add", "-A"]) {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Failed to stage files: {}", e.message)
        }));
    }

    match run_git(&project_path, &["commit", "-m", &message]) {
        Ok(_) => Ok(serde_json::json!({
            "success": true
        })),
        Err(e) => {
            if e.message.contains("nothing to commit")
                || e.message.contains("nothing added to commit")
            {
                Ok(serde_json::json!({
                    "success": false,
                    "error": "Nothing to commit - working tree clean"
                }))
            } else {
                Ok(serde_json::json!({
                    "success": false,
                    "error": format!("Commit failed: {}", e.message)
                }))
            }
        }
    }
}

#[tauri::command]
pub(crate) fn git_push(project_path: String) -> ApiResult<Value> {
    match run_git(&project_path, &["push"]) {
        Ok(_) => Ok(serde_json::json!({
            "success": true
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Push failed: {}", e.message)
        })),
    }
}

#[tauri::command]
pub(crate) fn git_pull(project_path: String) -> ApiResult<Value> {
    match run_git(&project_path, &["pull"]) {
        Ok(_) => Ok(serde_json::json!({
            "success": true
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Pull failed: {}", e.message)
        })),
    }
}

#[tauri::command]
pub(crate) fn git_reset(project_path: String, git_ref: Option<String>) -> ApiResult<Value> {
    let ref_target = git_ref.unwrap_or_else(|| "HEAD".to_string());

    match run_git(&project_path, &["reset", "--hard", &ref_target]) {
        Ok(_) => Ok(serde_json::json!({
            "success": true
        })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Reset failed: {}", e.message)
        })),
    }
}

#[tauri::command]
pub(crate) fn git_stage_file(project_path: String, file_path: String) -> ApiResult<Value> {
    match run_git(&project_path, &["add", "--", &file_path]) {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Stage failed: {}", e.message)
        })),
    }
}

#[tauri::command]
pub(crate) fn git_discard_file(
    project_path: String,
    file_path: String,
    is_untracked: bool,
) -> ApiResult<Value> {
    let result = if is_untracked {
        run_git(&project_path, &["clean", "-f", "--", &file_path])
    } else {
        let _ = run_git(&project_path, &["reset", "HEAD", "--", &file_path]);
        run_git(&project_path, &["checkout", "HEAD", "--", &file_path])
    };
    match result {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": format!("Discard failed: {}", e.message)
        })),
    }
}

#[tauri::command]
pub(crate) fn git_get_review_diffs(worktree_path: String) -> ApiResult<Value> {
    match build_review_diffs_for_path(&worktree_path) {
        Ok(v) => Ok(v),
        Err(e) => Ok(serde_json::json!({
            "success": false,
            "error": e.message,
            "files": Value::Array(vec![]),
            "summary": Value::Null
        })),
    }
}

#[tauri::command]
pub(crate) fn review_get_diffs_bundle(worktree_paths: Vec<String>) -> ApiResult<Value> {
    let mut out = Vec::new();
    for worktree_path in worktree_paths {
        match build_review_diffs_for_path(&worktree_path) {
            Ok(payload) => {
                out.push(serde_json::json!({
                  "worktreePath": worktree_path,
                  "success": true,
                  "files": payload.get("files").cloned().unwrap_or(Value::Array(vec![])),
                  "summary": payload.get("summary").cloned().unwrap_or(Value::Null)
                }));
            }
            Err(e) => {
                out.push(serde_json::json!({
                  "worktreePath": worktree_path,
                  "success": false,
                  "error": e.message,
                  "files": [],
                  "summary": Value::Null
                }));
            }
        }
    }
    Ok(Value::Array(out))
}
