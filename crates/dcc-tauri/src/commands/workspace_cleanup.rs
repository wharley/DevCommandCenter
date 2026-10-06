//! What deleting a completed task would cost and free.
//!
//! Deleting a task removes its worktree with `git worktree remove --force`;
//! its local branch stays in the repository. Uncommitted work is therefore
//! the thing a cleanup can lose for good, and commits that live only on the
//! task's branch are the thing a person may not remember is there. Each
//! worktree gets a verdict from local git state alone — no network, no
//! model — so the cleanup list can preselect only what is safe.

use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use dcc_core::{domain::workspace::WorkspaceId, ports::WorkspaceRepo};
use dcc_infra::db::SqliteWorkspaceRepo;

use crate::{
    commands::workspace_support::directory_logical_size, git::run_git_output,
    state::WorkspaceCommandState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceCleanupSafety {
    /// Clean worktree; every commit is on a remote or on the local base branch.
    Safe,
    /// Changes that were never committed, untracked files included. Lost on delete.
    Uncommitted,
    /// Commits only on the task's branch. Kept on the branch, but easy to forget.
    Unpushed,
    /// The task ran in the project checkout, or its folder is already gone.
    NoWorktree,
    /// Git could not inspect the worktree.
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupScanInput {
    pub workspace_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupEntry {
    pub workspace_id: String,
    pub bytes: u64,
    pub safety: WorkspaceCleanupSafety,
    /// Files with uncommitted changes, when `safety` is `uncommitted`.
    pub changed_files: u32,
    /// Commits on no remote and not on the base branch, when `safety` is `unpushed`.
    pub unpushed_commits: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCleanupScanOutput {
    pub workspaces: Vec<WorkspaceCleanupEntry>,
    /// Every distinct worktree counted once, even when tasks share one.
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Verdict {
    safety: WorkspaceCleanupSafety,
    changed_files: u32,
    unpushed_commits: u32,
}

impl Verdict {
    fn only(safety: WorkspaceCleanupSafety) -> Self {
        Self {
            safety,
            changed_files: 0,
            unpushed_commits: 0,
        }
    }
}

fn git_stdout(root: &str, args: &[&str]) -> Option<String> {
    run_git_output(root, args)
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Uncommitted changes win over unpushed commits: they are the ones a delete loses.
fn worktree_verdict(worktree: &Path, base_branch: &str) -> Verdict {
    if !worktree.exists() {
        return Verdict::only(WorkspaceCleanupSafety::NoWorktree);
    }
    let Some(root) = worktree.to_str() else {
        return Verdict::only(WorkspaceCleanupSafety::Unknown);
    };

    let Some(status) = git_stdout(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--no-renames",
            "--untracked-files=normal",
        ],
    ) else {
        return Verdict::only(WorkspaceCleanupSafety::Unknown);
    };
    let changed_files = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32;
    if changed_files > 0 {
        return Verdict {
            safety: WorkspaceCleanupSafety::Uncommitted,
            changed_files,
            unpushed_commits: 0,
        };
    }

    // Work merged into the local base branch without a push is not at risk.
    let base_ref = format!("refs/heads/{}", base_branch.trim());
    let has_base_ref = !base_branch.trim().is_empty()
        && git_stdout(root, &["rev-parse", "--verify", "--quiet", &base_ref]).is_some();
    let mut args = vec!["rev-list", "--count", "HEAD", "--not", "--remotes"];
    if has_base_ref {
        args.push(&base_ref);
    }
    let Some(unpushed_commits) =
        git_stdout(root, &args).and_then(|count| count.trim().parse::<u32>().ok())
    else {
        return Verdict::only(WorkspaceCleanupSafety::Unknown);
    };
    if unpushed_commits > 0 {
        return Verdict {
            safety: WorkspaceCleanupSafety::Unpushed,
            changed_files: 0,
            unpushed_commits,
        };
    }
    Verdict::only(WorkspaceCleanupSafety::Safe)
}

/// Size and safety of each task's own worktree. Unknown ids are skipped.
#[tauri::command]
pub async fn workspace_cleanup_scan(
    state: State<'_, WorkspaceCommandState>,
    input: WorkspaceCleanupScanInput,
) -> Result<WorkspaceCleanupScanOutput, String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let mut seen_ids = BTreeSet::new();
    let mut targets = Vec::new();
    for workspace_id in input.workspace_ids {
        if !seen_ids.insert(workspace_id.clone()) {
            continue;
        }
        let Some(workspace) = repo
            .get_workspace(&WorkspaceId(workspace_id.clone()))
            .await
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let worktree = workspace
            .worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty() && *path != workspace.root_path.trim())
            .map(PathBuf::from);
        targets.push((workspace_id, worktree, workspace.base_branch));
    }

    tauri::async_runtime::spawn_blocking(move || {
        // Two tasks can share a worktree path; measure and inspect it once.
        let mut measured = HashMap::<PathBuf, (u64, Verdict)>::new();
        let mut workspaces = Vec::with_capacity(targets.len());
        for (workspace_id, worktree, base_branch) in targets {
            let (bytes, verdict) = match worktree {
                None => (0, Verdict::only(WorkspaceCleanupSafety::NoWorktree)),
                Some(path) => match measured.get(&path) {
                    Some(known) => *known,
                    None => {
                        let known = (
                            directory_logical_size(&path)?,
                            worktree_verdict(&path, &base_branch),
                        );
                        measured.insert(path, known);
                        known
                    }
                },
            };
            workspaces.push(WorkspaceCleanupEntry {
                workspace_id,
                bytes,
                safety: verdict.safety,
                changed_files: verdict.changed_files,
                unpushed_commits: verdict.unpushed_commits,
            });
        }
        let total_bytes = measured
            .values()
            .map(|(bytes, _)| *bytes)
            .fold(0_u64, u64::saturating_add);
        Ok(WorkspaceCleanupScanOutput {
            workspaces,
            total_bytes,
        })
    })
    .await
    .map_err(|error| format!("failed to inspect completed worktrees: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=DCC", "-c", "user.email=dcc@example.com"])
            .args(args)
            .current_dir(root)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dcc-cleanup-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A repository with `main` pushed to a bare remote and a task worktree on `task`.
    fn repo_with_worktree(label: &str) -> (PathBuf, PathBuf) {
        let base = temp_dir(label);
        let remote = base.join("remote.git");
        let repo = base.join("repo");
        let worktree = base.join("task");
        fs::create_dir_all(&repo).unwrap();
        git(&base, &["init", "--bare", "-q", remote.to_str().unwrap()]);
        git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("README.md"), "hello\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(&repo, &["push", "-q", "origin", "main"]);
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task",
                worktree.to_str().unwrap(),
            ],
        );
        (base, worktree)
    }

    #[test]
    fn clean_worktree_with_everything_pushed_is_safe() {
        let (base, worktree) = repo_with_worktree("safe");
        assert_eq!(
            worktree_verdict(&worktree, "main"),
            Verdict::only(WorkspaceCleanupSafety::Safe)
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn untracked_and_modified_files_are_uncommitted() {
        let (base, worktree) = repo_with_worktree("dirty");
        fs::write(worktree.join("README.md"), "changed\n").unwrap();
        fs::write(worktree.join("notes.txt"), "draft\n").unwrap();
        let verdict = worktree_verdict(&worktree, "main");
        assert_eq!(verdict.safety, WorkspaceCleanupSafety::Uncommitted);
        assert_eq!(verdict.changed_files, 2);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn ignored_files_do_not_block_cleanup() {
        let (base, worktree) = repo_with_worktree("ignored");
        fs::write(worktree.join(".gitignore"), "node_modules/\n").unwrap();
        git(&worktree, &["add", ".gitignore"]);
        git(&worktree, &["commit", "-q", "-m", "ignore"]);
        git(&worktree, &["push", "-q", "origin", "task"]);
        fs::create_dir_all(worktree.join("node_modules/pkg")).unwrap();
        fs::write(worktree.join("node_modules/pkg/index.js"), "x").unwrap();
        assert_eq!(
            worktree_verdict(&worktree, "main").safety,
            WorkspaceCleanupSafety::Safe
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn commits_only_on_the_task_branch_are_unpushed() {
        let (base, worktree) = repo_with_worktree("unpushed");
        fs::write(worktree.join("feature.txt"), "work\n").unwrap();
        git(&worktree, &["add", "."]);
        git(&worktree, &["commit", "-q", "-m", "feature"]);
        let verdict = worktree_verdict(&worktree, "main");
        assert_eq!(verdict.safety, WorkspaceCleanupSafety::Unpushed);
        assert_eq!(verdict.unpushed_commits, 1);
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn commits_merged_into_the_local_base_branch_are_safe() {
        let (base, worktree) = repo_with_worktree("merged");
        fs::write(worktree.join("feature.txt"), "work\n").unwrap();
        git(&worktree, &["add", "."]);
        git(&worktree, &["commit", "-q", "-m", "feature"]);
        git(&base.join("repo"), &["merge", "-q", "--ff-only", "task"]);
        assert_eq!(
            worktree_verdict(&worktree, "main").safety,
            WorkspaceCleanupSafety::Safe
        );
        fs::remove_dir_all(base).ok();
    }

    #[test]
    fn a_missing_folder_frees_nothing() {
        let base = temp_dir("missing");
        assert_eq!(
            worktree_verdict(&base.join("gone"), "main").safety,
            WorkspaceCleanupSafety::NoWorktree
        );
        fs::remove_dir_all(base).ok();
    }
}
