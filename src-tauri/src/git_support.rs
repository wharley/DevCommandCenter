use dcc_tauri::git::{configure_git_command, parse_git_status_porcelain_z};
use serde_json::Value;
use std::process::Command;

use crate::{db_error, ApiResult};

pub(crate) fn run_git(cwd: &str, args: &[&str]) -> ApiResult<String> {
    let output = git_output_in_dir(cwd, args).map_err(|e| db_error(e.to_string()))?;
    if !output.status.success() {
        return Err(db_error(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Like run_git but accepts exit code 1 as success (git diff --no-index exits 1 when differences found).
pub(crate) fn run_git_diff(cwd: &str, args: &[&str]) -> ApiResult<String> {
    let output = git_output_in_dir(cwd, args).map_err(|e| db_error(e.to_string()))?;
    let code = output.status.code().unwrap_or(2);
    if code >= 2 {
        return Err(db_error(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn git_output_in_dir(cwd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command.args(args).current_dir(cwd).output()
}

fn count_diff_stats(diff: &str) -> (i64, i64) {
    let mut ins = 0i64;
    let mut del = 0i64;
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            ins += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            del += 1;
        }
    }
    (ins, del)
}

pub(crate) fn build_review_diffs_for_path(target_path: &str) -> ApiResult<Value> {
    let status_output = git_output_in_dir(target_path, &["status", "--porcelain", "-z"])
        .map_err(|e| db_error(e.to_string()))?;
    if !status_output.status.success() {
        return Err(db_error(
            String::from_utf8_lossy(&status_output.stderr).to_string(),
        ));
    }
    let files_meta = parse_git_status_porcelain_z(&status_output.stdout);
    let mut files = Vec::new();
    let mut insertions = 0i64;
    let mut deletions = 0i64;

    for file in &files_meta {
        let path = &file.path;
        let is_untracked = file.index_status == '?' && file.worktree_status == '?';
        let is_staged_new = !is_untracked && file.index_status == 'A';
        let diff = if is_untracked {
            run_git_diff(
                target_path,
                &["diff", "--binary", "--no-index", "--", "/dev/null", path],
            )
            .unwrap_or_default()
        } else if is_staged_new {
            run_git(target_path, &["diff", "--cached", "HEAD", "--", path]).unwrap_or_default()
        } else {
            run_git(target_path, &["diff", "HEAD", "--", path]).unwrap_or_default()
        };
        let (ins, del) = count_diff_stats(&diff);
        insertions += ins;
        deletions += del;
        let status = if is_untracked {
            "untracked"
        } else if file.index_status != ' ' {
            "staged"
        } else {
            "modified"
        };
        files.push(serde_json::json!({
            "path": path,
            "status": status,
            "diff": diff,
            "insertions": ins,
            "deletions": del
        }));
    }

    Ok(serde_json::json!({
      "success": true,
      "files": files,
      "summary": {
        "changedFiles": files_meta.len(),
        "insertions": insertions,
        "deletions": deletions
      }
    }))
}
