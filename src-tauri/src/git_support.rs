use dcc_tauri::git::{
    configure_git_command, parse_git_status_porcelain_z, split_null_terminated_fields,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

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

/// Caminho do ficheiro `index` do repositório para um worktree (dir `.git` ou ficheiro `gitdir:`).
pub(crate) fn resolve_git_index_path(worktree: &str) -> Option<PathBuf> {
    let wt = Path::new(worktree);
    let git_marker = wt.join(".git");
    if git_marker.is_dir() {
        return Some(git_marker.join("index"));
    }
    if git_marker.is_file() {
        let content = fs::read_to_string(&git_marker).ok()?;
        let rest = content.strip_prefix("gitdir:")?;
        let line = rest.trim();
        let gitdir = Path::new(line);
        let resolved = if gitdir.is_absolute() {
            gitdir.to_path_buf()
        } else {
            wt.join(gitdir)
        };
        return Some(resolved.join("index"));
    }
    None
}

pub(crate) fn git_parse_first_unix_ts(worktree: &str, args: &[&str]) -> Option<i64> {
    let output = git_output_in_dir(worktree, args).ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let line = s.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    line.parse().ok()
}

/// Heurística: max(reflog HEAD, último commit, mtime do index se working tree dirty).
pub(crate) fn git_worktree_last_activity_epoch(worktree: &str) -> Option<i64> {
    let mut max_ts: Option<i64> = None;
    let mut bump = |t: i64| {
        max_ts = Some(match max_ts {
            Some(m) => m.max(t),
            None => t,
        });
    };

    if let Some(t) = git_parse_first_unix_ts(worktree, &["reflog", "-1", "--format=%ct", "HEAD"]) {
        bump(t);
    }
    if let Some(t) = git_parse_first_unix_ts(worktree, &["log", "-1", "--format=%ct"]) {
        bump(t);
    }

    let dirty = git_output_in_dir(worktree, &["status", "--porcelain"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    if dirty {
        if let Some(index_path) = resolve_git_index_path(worktree) {
            if let Ok(meta) = fs::metadata(&index_path) {
                if let Ok(st) = meta.modified() {
                    if let Ok(dur) = st.duration_since(UNIX_EPOCH) {
                        bump(dur.as_secs() as i64);
                    }
                }
            }
        }
    }

    if max_ts.is_none() {
        if let Some(index_path) = resolve_git_index_path(worktree) {
            if let Ok(meta) = fs::metadata(&index_path) {
                if let Ok(st) = meta.modified() {
                    if let Ok(dur) = st.duration_since(UNIX_EPOCH) {
                        return Some(dur.as_secs() as i64);
                    }
                }
            }
        }
    }

    max_ts
}

/// Reverte `git worktree add` + branch local após falha do setup (best-effort, alinhado a `comb_discard`).
pub(crate) fn git_remove_worktree_and_branch_best_effort(
    project_path: &str,
    worktree_path: &str,
    branch: &str,
) {
    let _ = run_git(
        project_path,
        &["worktree", "remove", "--force", worktree_path],
    );
    let _ = run_git(project_path, &["branch", "-D", branch]);
}

pub(crate) fn git_worktree_list_contains_path(project_path: &str, worktree_path: &str) -> bool {
    let desired = Path::new(worktree_path);
    let desired = fs::canonicalize(desired)
        .unwrap_or_else(|_| desired.to_path_buf())
        .to_string_lossy()
        .to_string();

    let output = match git_output_in_dir(project_path, &["worktree", "list", "--porcelain"]) {
        Ok(output) => output,
        Err(_) => return false,
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(raw_path) = line.strip_prefix("worktree ") {
            let path = Path::new(raw_path.trim());
            let path = fs::canonicalize(path)
                .unwrap_or_else(|_| path.to_path_buf())
                .to_string_lossy()
                .to_string();
            if path == desired {
                return true;
            }
        }
    }

    false
}

/// Branches locais (`refs/heads/`), ordenadas, sem duplicatas.
pub(crate) fn git_local_branch_names(project_path: &str) -> ApiResult<Vec<String>> {
    let output = git_output_in_dir(
        project_path,
        &[
            "for-each-ref",
            "--format=%(refname:short)%00",
            "refs/heads/",
        ],
    )
    .map_err(|e| db_error(e.to_string()))?;
    if !output.status.success() {
        return Err(db_error(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let mut branches = split_null_terminated_fields(&output.stdout);
    branches.sort();
    branches.dedup();
    Ok(branches)
}

pub(crate) fn git_current_branch_impl(project_path: &str) -> ApiResult<String> {
    Ok(
        run_git(project_path, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string(),
    )
}

pub(crate) fn get_unpushed_commits(cwd: &str, branch: &str) -> ApiResult<Vec<String>> {
    let upstream = run_git(
        cwd,
        &["rev-parse", "--abbrev-ref", &format!("{}@{{u}}", branch)],
    )
    .unwrap_or_default()
    .trim()
    .to_string();

    if upstream.is_empty() {
        let log = run_git(cwd, &["log", "--oneline", "HEAD"]).unwrap_or_default();
        return Ok(log.lines().map(|s| s.to_string()).collect());
    }

    let log = run_git(cwd, &["log", "--oneline", &format!("{}..HEAD", upstream)])?;
    Ok(log.lines().map(|s| s.to_string()).collect())
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
