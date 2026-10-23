//! The remote branch a deletion may remove, and the DCC remote a deleted
//! task may leave unused.

use std::collections::BTreeSet;
use std::path::Path;

use dcc_core::domain::workspace::{Workspace, WorkspacePushTarget};
pub use dcc_core::domain::workspace_git::WorkspaceRemoteBranchDeletionTarget;
use dcc_core::ports::WorkspaceRepo;
use url::Url;

use super::delegation::validate_branch_for_fetch;
use super::DeletionRuntime;
use crate::db::SqliteWorkspaceRepo;
use crate::git_command::{git_output_err, run_git_output};
use crate::workspace_git::{
    resolve_current_branch_name, resolve_current_commit_sha, resolve_workspace_active_root,
};

pub fn git_branch_config_uses_remote(root: &str, remote_name: &str) -> bool {
    let Ok(output) = run_git_output(
        root,
        &[
            "config",
            "--get-regexp",
            r"^branch\..*\.(remote|pushRemote)$",
        ],
    ) else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .any(|value| value == remote_name)
}

pub async fn unused_workspace_push_target(
    repo: &SqliteWorkspaceRepo,
    removed: &Workspace,
    retiring_workspace_ids: &BTreeSet<String>,
) -> Result<Option<WorkspacePushTarget>, String> {
    let Some(target) = removed
        .source
        .as_ref()
        .and_then(|source| source.push_target.as_ref())
    else {
        return Ok(None);
    };
    if !target.remote_created
        || target.remote_url.is_none()
        || matches!(target.remote_name.as_str(), "origin" | "upstream")
    {
        return Ok(None);
    }
    let used_by_another_workspace = repo
        .list_workspaces()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|workspace| {
            workspace.id != removed.id
                && !retiring_workspace_ids.contains(&workspace.id.0)
                && workspace.root_path == removed.root_path
        })
        .filter_map(|workspace| workspace.source?.push_target)
        .any(|known| {
            known.remote_name == target.remote_name
                || known.remote_url.as_deref() == target.remote_url.as_deref()
        });
    if used_by_another_workspace {
        return Ok(None);
    }
    Ok(Some(target.clone()))
}

pub fn cleanup_unused_workspace_push_target_at_root(
    root: &str,
    target: &WorkspacePushTarget,
) -> Result<(), String> {
    if git_branch_config_uses_remote(root, &target.remote_name) {
        return Ok(());
    }
    let output = run_git_output(root, &["remote", "get-url", &target.remote_name])?;
    if !output.status.success() {
        return Ok(());
    }
    let configured_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if target.remote_url.as_deref() != Some(configured_url.as_str()) {
        return Ok(());
    }
    let output = run_git_output(root, &["remote", "remove", &target.remote_name])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_output_err("git remote remove", &output.stderr))
    }
}

pub async fn cleanup_unused_workspace_push_target<R: DeletionRuntime>(
    runtime: &R,
    repo: &SqliteWorkspaceRepo,
    removed: &Workspace,
    retiring_workspace_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let Some(target) = unused_workspace_push_target(repo, removed, retiring_workspace_ids).await?
    else {
        return Ok(());
    };
    let active_root = resolve_workspace_active_root(removed).to_string();
    if !Path::new(&active_root).exists() {
        // This is optional shared-repository configuration cleanup. A prior
        // partial deletion may already have removed the worktree; retaining a
        // now-unused DCC remote is safer than blocking permanent workspace
        // cleanup on a physical root that no longer exists.
        return Ok(());
    }
    runtime
        .git_mutation(&active_root, move |trusted_root| {
            let root = trusted_root
                .to_str()
                .ok_or_else(|| "workspace root is not valid UTF-8".to_string())?;
            cleanup_unused_workspace_push_target_at_root(root, &target)
        })
        .await
}

pub fn workspace_remote_branch_target_at_root(
    workspace: &Workspace,
    active_root: &str,
) -> Result<Option<(String, String)>, String> {
    let source = workspace.source.as_ref();
    let source_branch = source
        .and_then(|source| {
            source
                .push_target
                .as_ref()
                .map(|target| target.branch_name.as_str())
        })
        .or_else(|| source.map(|source| source.head_branch.as_str()))
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    let current_branch = resolve_current_branch_name(active_root)
        .ok()
        .filter(|branch| branch != "HEAD");
    let branch = current_branch
        .as_deref()
        .or(source_branch)
        .unwrap_or_default()
        .trim();
    if branch.is_empty() {
        return Ok(None);
    }
    if workspace.base_branch.trim() == branch
        || source.is_some_and(|source| source.base_branch.trim() == branch)
    {
        return Err(format!(
            "Refusing to delete `{branch}` because it is the workspace base branch."
        ));
    }
    if matches!(branch, "main" | "master" | "trunk") {
        return Err(format!("Refusing to delete protected branch `{branch}`."));
    }

    let configured_remote = |key: &str| -> Option<String> {
        let output = run_git_output(active_root, &["config", "--get", key]).ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!value.is_empty() && value != ".").then_some(value)
    };
    let source_remote = source
        .filter(|_| source_branch == Some(branch))
        .and_then(|source| {
            source
                .push_target
                .as_ref()
                .map(|target| target.remote_name.trim())
                .filter(|remote| !remote.is_empty())
                .or_else(|| {
                    let remote = source.remote_name.trim();
                    (!remote.is_empty()).then_some(remote)
                })
        })
        .map(str::to_string);
    // Imported branch/PR workspaces have an explicit push target recorded by DCC.
    // For regular worktree branches, follow Git's push-remote precedence so the
    // branch is removed from the same remote it would normally be pushed to.
    let remote = source_remote
        .or_else(|| configured_remote(&format!("branch.{branch}.pushRemote")))
        .or_else(|| configured_remote("remote.pushDefault"))
        .or_else(|| configured_remote(&format!("branch.{branch}.remote")));

    let Some(remote) = remote else {
        // Having an `origin` remote alone does not prove that this local branch
        // was published there. Without an explicit push/tracking destination,
        // do not offer remote deletion.
        return Ok(None);
    };

    Ok(Some((remote, branch.to_string())))
}

pub fn validate_remote_branch_deletion_oid(oid: &str) -> Result<String, String> {
    let oid = oid.trim();
    let valid_length = matches!(oid.len(), 40 | 64);
    if !valid_length || !oid.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(
            "The remote branch confirmation is incomplete. Reopen the deletion dialog and try again."
                .to_string(),
        );
    }
    Ok(oid.to_ascii_lowercase())
}

pub fn observed_workspace_push_url(root: &str, remote: &str) -> Result<(String, String), String> {
    let output = run_git_output(root, &["remote", "get-url", "--push", "--all", remote])?;
    if !output.status.success() {
        return Err(git_output_err(
            "git remote get-url --push --all",
            &output.stderr,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let push_urls = stdout
        .lines()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .collect::<Vec<_>>();
    if push_urls.len() != 1 {
        return Err(
            "Remote branch deletion requires exactly one push URL. Review the remote configuration and try again."
                .to_string()
        );
    }
    let raw = push_urls[0].to_string();
    Ok((raw.clone(), redact_push_route_credentials(&raw)))
}

pub fn push_url_contains_credentials(raw: &str) -> bool {
    Url::parse(raw).is_ok_and(|url| {
        url.password().is_some()
            || (matches!(url.scheme(), "http" | "https") && !url.username().is_empty())
    })
}

pub fn remote_branch_oid(root: &str, remote: &str, branch: &str) -> Result<Option<String>, String> {
    let remote_ref = format!("refs/heads/{branch}");
    let output = run_git_output(root, &["ls-remote", "--heads", remote, &remote_ref])?;
    if !output.status.success() {
        return Err(git_output_err("git ls-remote --heads", &output.stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(line) = stdout.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Ok(None);
    };
    let Some((oid, reference)) = line.split_once(char::is_whitespace) else {
        return Err("Git returned an invalid remote branch identity.".to_string());
    };
    if reference.trim() != remote_ref {
        return Err("Git returned an unexpected remote branch identity.".to_string());
    }
    validate_remote_branch_deletion_oid(oid).map(Some)
}

pub fn workspace_remote_branch_deletion_target(
    workspace: &Workspace,
) -> Result<Option<WorkspaceRemoteBranchDeletionTarget>, String> {
    let active_root = resolve_workspace_active_root(workspace);
    let Some((remote, branch)) = workspace_remote_branch_target_at_root(workspace, active_root)?
    else {
        return Ok(None);
    };
    validate_branch_for_fetch(active_root, &branch)?;
    let expected_oid = resolve_current_commit_sha(active_root)?.ok_or_else(|| {
        "The worktree HEAD is unavailable. Reopen the deletion dialog and try again.".to_string()
    })?;
    let expected_oid = validate_remote_branch_deletion_oid(&expected_oid)?;
    let (_, push_url) = observed_workspace_push_url(active_root, &remote)?;
    Ok(Some(WorkspaceRemoteBranchDeletionTarget {
        remote,
        branch,
        expected_oid,
        push_url,
    }))
}

pub fn remote_default_branch(root: &str, remote: &str) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["ls-remote", "--symref", remote, "HEAD"])?;
    if !output.status.success() {
        return Err(git_output_err("git ls-remote --symref", &output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("ref: refs/heads/")
                .and_then(|rest| rest.split_whitespace().next())
                .map(str::to_string)
        }))
}

pub fn delete_workspace_remote_branch_target(
    active_root: &str,
    remote: &str,
    branch: &str,
    expected_oid: &str,
    expected_push_url: &str,
) -> Result<(), String> {
    validate_branch_for_fetch(active_root, branch)?;
    let expected_oid = validate_remote_branch_deletion_oid(expected_oid)?;
    if expected_push_url.trim().is_empty() {
        return Err(
            "The remote branch confirmation is incomplete. Reopen the deletion dialog and try again."
                .to_string(),
        );
    }
    let local_oid = resolve_current_commit_sha(active_root)?.ok_or_else(|| {
        "The worktree HEAD is unavailable. Reopen the deletion dialog and try again.".to_string()
    })?;
    if !local_oid.eq_ignore_ascii_case(&expected_oid) {
        return Err(
            "The worktree HEAD changed after the deletion dialog opened. Reopen the deletion dialog and try again."
                .to_string(),
        );
    }
    delete_confirmed_remote_branch_at_root(
        active_root,
        remote,
        branch,
        &expected_oid,
        expected_push_url,
    )
}

pub fn delete_confirmed_remote_branch_at_root(
    root: &str,
    remote: &str,
    branch: &str,
    expected_oid: &str,
    expected_push_url: &str,
) -> Result<(), String> {
    let (raw_push_url, observed_push_url) = observed_workspace_push_url(root, remote)?;
    if push_url_contains_credentials(&raw_push_url) {
        return Err(
            "The remote push URL contains embedded credentials. Remove them and use your credential manager before deleting a branch."
                .to_string(),
        );
    }
    if observed_push_url != expected_push_url.trim() {
        return Err(
            "The remote push URL changed after the deletion dialog opened. Reopen the deletion dialog and try again."
                .to_string(),
        );
    }
    if remote_default_branch(root, &raw_push_url)?.as_deref() == Some(branch) {
        return Err(format!(
            "Refusing to delete `{branch}` because it is the default branch on `{remote}`."
        ));
    }

    let Some(observed_remote_oid) = remote_branch_oid(root, &raw_push_url, branch)? else {
        return Ok(());
    };
    if observed_remote_oid != expected_oid {
        return Err(
            "The remote branch does not match the confirmed worktree commit. Fetch and review the branch before trying again."
                .to_string(),
        );
    }

    let remote_ref = format!("refs/heads/{branch}");
    let lease = format!("--force-with-lease={remote_ref}:{expected_oid}");
    let delete_refspec = format!(":{remote_ref}");
    let output = run_git_output(root, &["push", &lease, remote, &delete_refspec])?;
    if !output.status.success() {
        return Err(git_output_err(
            "git push --force-with-lease",
            &output.stderr,
        ));
    }
    Ok(())
}

pub fn delete_workspace_remote_branch_after_worktree_loss(
    repository_root: &str,
    workspace: &Workspace,
    expected: &WorkspaceRemoteBranchDeletionTarget,
) -> Result<(), String> {
    let remote = expected.remote.trim();
    let branch = expected.branch.trim();
    if remote.is_empty() || branch.is_empty() {
        return Err(
            "The remote branch confirmation is incomplete. Reopen the deletion dialog and try again."
                .to_string(),
        );
    }
    validate_branch_for_fetch(repository_root, branch)?;
    if workspace.base_branch.trim() == branch || matches!(branch, "main" | "master" | "trunk") {
        return Err(format!("Refusing to delete protected branch `{branch}`."));
    }
    let expected_oid = validate_remote_branch_deletion_oid(&expected.expected_oid)?;
    let local_ref = format!("refs/heads/{branch}");
    let local_oid = resolve_commitish_sha(repository_root, &local_ref)?;
    if !local_oid.eq_ignore_ascii_case(&expected_oid) {
        return Err(
            "The saved local branch no longer matches the confirmed worktree commit. Review the branch before trying again."
                .to_string(),
        );
    }
    delete_confirmed_remote_branch_at_root(
        repository_root,
        remote,
        branch,
        &expected_oid,
        &expected.push_url,
    )
}

pub fn delete_workspace_remote_branch(
    workspace: &Workspace,
    active_root: &str,
    expected: &WorkspaceRemoteBranchDeletionTarget,
) -> Result<(), String> {
    let Some((remote, branch)) = workspace_remote_branch_target_at_root(workspace, active_root)?
    else {
        return Err("The worktree no longer has a remote branch to delete. Review the deletion confirmation and try again.".to_string());
    };
    if branch != expected.branch.trim() || remote != expected.remote.trim() {
        return Err(format!(
            "The remote branch changed from `{}/{}` to `{remote}/{branch}`. Review the deletion confirmation and try again.",
            expected.remote.trim(),
            expected.branch.trim()
        ));
    }
    delete_workspace_remote_branch_target(
        active_root,
        &remote,
        &branch,
        &expected.expected_oid,
        &expected.push_url,
    )
}

pub fn redact_push_route_credentials(raw: &str) -> String {
    let mut redacted = raw.to_string();
    let mut cursor = 0usize;
    while let Some(relative_scheme) = redacted[cursor..].find("://") {
        let authority_start = cursor + relative_scheme + 3;
        let authority_end = redacted[authority_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '/' | '?' | '#')
            })
            .map(|relative| authority_start + relative)
            .unwrap_or(redacted.len());
        let Some(relative_at) = redacted[authority_start..authority_end].rfind('@') else {
            cursor = authority_end;
            continue;
        };
        let at = authority_start + relative_at;
        redacted.replace_range(authority_start..at, "[redacted]");
        cursor = authority_start + "[redacted]@".len();
    }
    redacted
}

pub fn resolve_commitish_sha(root: &str, commitish: &str) -> Result<String, String> {
    let revision = format!("{commitish}^{{commit}}");
    let output = run_git_output(root, &["rev-parse", "--verify", &revision])?;
    if !output.status.success() {
        return Err("could not resolve the requested branch commit".to_string());
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if commit.is_empty() {
        return Err("resolved branch commit is empty".to_string());
    }
    Ok(commit)
}
