//! What surrounds a pull request's merge on GitHub: merging when it becomes
//! ready (GitHub's own auto-merge), turning that off, bringing its branch
//! up to date with the base, closing and reopening it, and deleting the head
//! branch. The branch is deleted through the API, never with
//! `gh pr merge --delete-branch`, so no local checkout is ever touched.

use std::process::{Command, Stdio};

use super::github::resolve_auth_context;
use super::resolve_cli_binary;

/// `--auto` asks GitHub to merge as soon as the requirements are met; it
/// merges at once when they already are.
pub fn merge_flags(when_ready: bool) -> &'static [&'static str] {
    if when_ready {
        &["--auto"]
    } else {
        &[]
    }
}

/// The API path of a branch's ref; none for a name that is not a branch
/// (empty, or carrying what a path must not).
pub fn branch_ref_endpoint(namespace: &str, repo: &str, branch: &str) -> Option<String> {
    let branch = branch.trim();
    let plain = !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.contains("..")
        && branch
            .chars()
            .all(|c| !c.is_whitespace() && !matches!(c, '?' | '#' | '%' | '\\' | '~' | '^' | ':'));
    plain.then(|| format!("/repos/{namespace}/{repo}/git/refs/heads/{branch}"))
}

fn run(root: &str, host: &str, login: Option<&str>, args: &[&str]) -> Result<(), String> {
    let gh = resolve_cli_binary("gh")?;
    let auth = resolve_auth_context(host, login)?;
    let mut command = Command::new(gh);
    command
        .current_dir(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(auth) = auth.as_ref() {
        command.envs(auth.envs.iter().map(|(key, value)| (key, value)));
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let detail = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Err(if detail.trim().is_empty() {
        "GitHub did not accept the request.".to_string()
    } else {
        detail.trim().to_string()
    })
}

/// Deletes the merged pull request's branch on GitHub.
pub fn delete_head_branch(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    branch: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let endpoint = branch_ref_endpoint(namespace, repo, branch)
        .ok_or_else(|| "The pull request's branch name is not valid.".to_string())?;
    run(
        root,
        host,
        login,
        &["api", "--hostname", host, "-X", "DELETE", &endpoint],
    )
}

/// Turns off "merge when ready" on a pull request.
pub fn disable_auto_merge(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    number: u32,
    login: Option<&str>,
) -> Result<(), String> {
    let repository = format!("{host}/{namespace}/{repo}");
    run(
        root,
        host,
        login,
        &[
            "pr",
            "merge",
            &number.to_string(),
            "--repo",
            &repository,
            "--disable-auto",
        ],
    )
}

/// Merges the base into the pull request's branch on GitHub, only while
/// its head is still the commit the person saw.
pub fn update_branch(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    number: u32,
    expected_head_sha: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let endpoint = format!("/repos/{namespace}/{repo}/pulls/{number}/update-branch");
    let expected = format!("expected_head_sha={expected_head_sha}");
    run(
        root,
        host,
        login,
        &[
            "api",
            "--hostname",
            host,
            "-X",
            "PUT",
            &endpoint,
            "-f",
            &expected,
        ],
    )
}

/// `gh pr close`, with the person's comment when there is one.
pub fn close_args<'a>(
    number: &'a str,
    repository: &'a str,
    comment: Option<&'a str>,
) -> Vec<&'a str> {
    let mut args = vec!["pr", "close", number, "--repo", repository];
    if let Some(comment) = comment.map(str::trim).filter(|comment| !comment.is_empty()) {
        args.extend(["--comment", comment]);
    }
    args
}

/// Closes a pull request without merging it.
pub fn close_pull_request(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    number: u32,
    comment: Option<&str>,
    login: Option<&str>,
) -> Result<(), String> {
    let repository = format!("{host}/{namespace}/{repo}");
    let number = number.to_string();
    run(
        root,
        host,
        login,
        &close_args(&number, &repository, comment),
    )
}

pub fn reopen_pull_request(
    root: &str,
    host: &str,
    namespace: &str,
    repo: &str,
    number: u32,
    login: Option<&str>,
) -> Result<(), String> {
    let repository = format!("{host}/{namespace}/{repo}");
    run(
        root,
        host,
        login,
        &["pr", "reopen", &number.to_string(), "--repo", &repository],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_ref_is_built_only_for_a_plain_branch_name() {
        assert_eq!(
            branch_ref_endpoint("acme", "shop", "feat/pay-1").as_deref(),
            Some("/repos/acme/shop/git/refs/heads/feat/pay-1")
        );
        for bad in ["", " ", "-x", "a..b", "a b", "a?b", "a#b", "a:b", "a%2f"] {
            assert_eq!(branch_ref_endpoint("acme", "shop", bad), None, "{bad}");
        }
    }

    #[test]
    fn closing_carries_a_comment_only_when_there_is_one() {
        assert_eq!(
            close_args("7", "github.com/acme/shop", Some("  superseded by #9 ")),
            [
                "pr",
                "close",
                "7",
                "--repo",
                "github.com/acme/shop",
                "--comment",
                "superseded by #9"
            ]
        );
        assert_eq!(close_args("7", "r", Some("  ")).len(), 5);
        assert_eq!(close_args("7", "r", None).len(), 5);
    }

    #[test]
    fn merging_when_ready_is_githubs_auto_merge() {
        assert_eq!(merge_flags(true), ["--auto"]);
        assert!(merge_flags(false).is_empty());
    }
}
