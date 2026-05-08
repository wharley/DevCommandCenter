use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use dcc_core::{
    ports::{ClonedRepository, GitOps, PreparedWorktree},
    CoreError, Result,
};

#[derive(Clone, Debug, Default)]
pub struct CommandGitOps;

const GIT_LOCAL_TIMEOUT: Duration = Duration::from_secs(15);
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(300);
const GIT_HARDENED_CONFIG_ARGS: [&str; 4] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSetupSuggestion {
    pub label: String,
    pub command: String,
    pub source_path: String,
}

impl CommandGitOps {
    pub fn new() -> Self {
        Self
    }
}

fn apply_non_interactive_git_env(command: &mut Command) {
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GCM_INTERACTIVE", "Never");
    command.env_remove("GIT_ASKPASS");
    command.env_remove("SSH_ASKPASS");

    let base_ssh = std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh".to_string());
    command.env(
        "GIT_SSH_COMMAND",
        format!("{base_ssh} -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=yes"),
    );
}

fn git_output_detail(output: &Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    fallback.to_string()
}

fn run_git_with_timeout<I>(args: I, current_dir: Option<&Path>, timeout: Duration) -> Result<Output>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command.args(GIT_HARDENED_CONFIG_ARGS);
    for arg in args {
        command.arg(arg.as_ref());
    }
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }

    apply_non_interactive_git_env(&mut command);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let child = command
        .spawn()
        .map_err(|error| CoreError::Git(error.to_string()))?;
    let child_pid = child.id();

    let (tx, rx) = mpsc::channel();
    let waiter = thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let _ = waiter.join();
            Ok(output)
        }
        Ok(Err(error)) => {
            let _ = waiter.join();
            Err(CoreError::Git(error.to_string()))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
            }
            #[cfg(windows)]
            {
                let child_pid = child_pid.to_string();
                let _ = Command::new("taskkill")
                    .arg("/PID")
                    .arg(&child_pid)
                    .arg("/T")
                    .arg("/F")
                    .output();
            }
            let _ = waiter.join();
            Err(CoreError::Git(format!(
                "git command timed out after {}s",
                timeout.as_secs()
            )))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = waiter.join();
            Err(CoreError::Git(
                "git waiter thread crashed before returning output".to_string(),
            ))
        }
    }
}

fn run_git_stdout<I>(
    args: I,
    current_dir: Option<&Path>,
    timeout: Duration,
    fallback: &str,
) -> Result<String>
where
    I: IntoIterator,
    I::Item: AsRef<OsStr>,
{
    let output = run_git_with_timeout(args, current_dir, timeout)?;
    if !output.status.success() {
        return Err(CoreError::Git(git_output_detail(&output, fallback)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn sanitize_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else {
            output.push('-');
        }
    }
    let trimmed = output.trim_matches('-').trim_matches('.');
    if trimmed.is_empty() {
        "workspace".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn build_worktree_path(workspace_root: &Path, base_branch: &str) -> PathBuf {
    let repo_parent = workspace_root.parent().unwrap_or(workspace_root);
    let worktrees_root = repo_parent.join(".dcc-worktrees");
    worktrees_root.join(format!(
        "{}-{}",
        sanitize_segment(base_branch),
        Uuid::new_v4().simple()
    ))
}

pub fn is_git_repo(workspace_root: &Path) -> bool {
    run_git_with_timeout(
        ["rev-parse", "--is-inside-work-tree"],
        Some(workspace_root),
        GIT_LOCAL_TIMEOUT,
    )
    .map(|output| output.status.success())
    .unwrap_or(false)
}

fn parse_local_branch_names(output: &str) -> Vec<String> {
    let mut branches: Vec<String> = output
        .split('\0')
        .flat_map(|line| line.lines())
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    branches.sort();
    branches.dedup();
    branches
}

fn parse_default_branch_from_ls_remote_output(output: &str) -> Option<String> {
    for line in output.lines() {
        let Some((ref_part, remainder)) = line.split_once('\t') else {
            continue;
        };

        if remainder.trim() != "HEAD" {
            continue;
        }

        let Some(branch) = ref_part.strip_prefix("ref: refs/heads/") else {
            continue;
        };

        let branch = branch.trim();
        if !branch.is_empty() {
            return Some(branch.to_string());
        }
    }

    None
}

fn detect_default_branch(repository_url: &str) -> Result<String> {
    let stdout = run_git_stdout(
        ["ls-remote", "--symref", repository_url, "HEAD"],
        None,
        GIT_NETWORK_TIMEOUT,
        "failed to detect default branch",
    )?;
    parse_default_branch_from_ls_remote_output(&stdout)
        .ok_or_else(|| CoreError::Git("failed to detect default branch from remote".to_string()))
}

pub fn is_broken_worktree_error_text(message: &str) -> bool {
    message.contains("not a git repository")
        || message.contains("is not a working tree")
        || message.contains("cannot change to")
        || message.contains("No such file or directory")
}

pub fn broken_worktree_reason(workspace_root: &Path) -> Option<String> {
    if !workspace_root.exists() {
        return Some(format!(
            "workspace path no longer exists: {}",
            workspace_root.display()
        ));
    }

    match run_git_with_timeout(
        ["rev-parse", "--is-inside-work-tree"],
        Some(workspace_root),
        GIT_LOCAL_TIMEOUT,
    ) {
        Ok(output) if output.status.success() => None,
        Ok(output) => {
            let detail = git_output_detail(&output, "failed to inspect git worktree");
            if is_broken_worktree_error_text(&detail) {
                Some(detail)
            } else {
                None
            }
        }
        Err(CoreError::Git(message)) if is_broken_worktree_error_text(&message) => Some(message),
        Err(_) => None,
    }
}

pub fn list_local_branch_names(project_path: &str) -> Result<Vec<String>> {
    let project_path = Path::new(project_path);

    if !project_path.exists() {
        return Err(CoreError::Git(format!(
            "repository path does not exist: {}",
            project_path.display()
        )));
    }

    if !is_git_repo(project_path) {
        return Err(CoreError::Git(format!(
            "not a git repository: {}",
            project_path.display()
        )));
    }

    let stdout = run_git_stdout(
        [
            "for-each-ref",
            "--format=%(refname:short)%00",
            "refs/heads/",
        ],
        Some(project_path),
        GIT_LOCAL_TIMEOUT,
        "failed to list local branches",
    )?;
    Ok(parse_local_branch_names(&stdout))
}

pub fn detect_workspace_setup_suggestions(workspace_root: &str) -> Vec<WorkspaceSetupSuggestion> {
    let workspace_root = Path::new(workspace_root);
    if !workspace_root.exists() {
        return Vec::new();
    }

    let mut suggestions = Vec::new();
    let package_json = workspace_root.join("package.json");
    if package_json.is_file() {
        let command = if workspace_root.join("pnpm-lock.yaml").is_file() {
            "pnpm install"
        } else if workspace_root.join("yarn.lock").is_file() {
            "yarn install"
        } else if workspace_root.join("bun.lock").is_file()
            || workspace_root.join("bun.lockb").is_file()
        {
            "bun install"
        } else {
            "npm install"
        };

        suggestions.push(WorkspaceSetupSuggestion {
            label: "Install JavaScript dependencies".to_string(),
            command: command.to_string(),
            source_path: package_json.to_string_lossy().to_string(),
        });
    }

    let cargo_toml = workspace_root.join("Cargo.toml");
    if cargo_toml.is_file() {
        suggestions.push(WorkspaceSetupSuggestion {
            label: "Build Rust workspace".to_string(),
            command: "cargo build".to_string(),
            source_path: cargo_toml.to_string_lossy().to_string(),
        });
    }

    suggestions
}

pub fn create_worktree_branch_from_ref(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    start_point: &str,
) -> Result<()> {
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
    }

    run_git_stdout(
        ["worktree", "prune"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git worktree prune failed",
    )?;

    let args = vec![
        OsString::from("worktree"),
        OsString::from("add"),
        OsString::from("-b"),
        OsString::from(branch),
        worktree_path.as_os_str().to_os_string(),
        OsString::from(start_point),
    ];
    run_git_stdout(
        args,
        Some(repo_root),
        GIT_NETWORK_TIMEOUT,
        "git worktree add failed",
    )?;
    Ok(())
}

pub fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> Result<()> {
    let args = vec![
        OsString::from("worktree"),
        OsString::from("remove"),
        OsString::from("--force"),
        worktree_path.as_os_str().to_os_string(),
    ];
    run_git_stdout(
        args,
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git worktree remove failed",
    )?;
    Ok(())
}

pub fn remove_branch(repo_root: &Path, branch: &str) -> Result<()> {
    run_git_stdout(
        ["branch", "-D", branch],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git branch -D failed",
    )?;
    Ok(())
}

pub fn stash_create(repo_root: &Path) -> Result<Option<String>> {
    let output = run_git_stdout(
        ["stash", "create"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
        "git stash create failed",
    )?;
    let sha = output.trim();
    if sha.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sha.to_string()))
    }
}

pub fn stash_apply_sha(workspace_dir: &Path, stash_sha: &str) -> Result<()> {
    run_git_stdout(
        ["stash", "apply", stash_sha],
        Some(workspace_dir),
        GIT_LOCAL_TIMEOUT,
        "git stash apply failed",
    )?;
    Ok(())
}

pub fn list_untracked_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = run_git_with_timeout(
        ["ls-files", "--others", "--exclude-standard", "-z"],
        Some(repo_root),
        GIT_LOCAL_TIMEOUT,
    )?;
    if !output.status.success() {
        return Err(CoreError::Git(git_output_detail(
            &output,
            "git ls-files failed",
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths = stdout
        .split('\0')
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

#[async_trait]
impl GitOps for CommandGitOps {
    async fn clone_repository(
        &self,
        repository_url: &str,
        destination_path: &str,
        base_branch: &str,
    ) -> Result<ClonedRepository> {
        let repository_url = repository_url.trim();
        let base_branch = base_branch.trim();
        let destination_path = Path::new(destination_path);

        if repository_url.is_empty() {
            return Err(CoreError::Git("repository URL cannot be empty".to_string()));
        }

        let resolved_branch = if base_branch.is_empty() {
            detect_default_branch(repository_url)?
        } else {
            base_branch.to_string()
        };

        if let Some(parent) = destination_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
        }

        let clone_args = vec![
            OsString::from("clone"),
            OsString::from("--branch"),
            OsString::from(&resolved_branch),
            OsString::from("--single-branch"),
            OsString::from(repository_url),
            destination_path.as_os_str().to_os_string(),
        ];

        run_git_stdout(clone_args, None, GIT_CLONE_TIMEOUT, "git clone failed")?;

        Ok(ClonedRepository {
            path: destination_path.to_string_lossy().to_string(),
            branch: resolved_branch,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    async fn prepare_worktree(
        &self,
        workspace_root: &str,
        base_branch: &str,
    ) -> Result<PreparedWorktree> {
        let workspace_root = Path::new(workspace_root);
        if !workspace_root.exists() {
            return Err(CoreError::Git(format!(
                "workspace root does not exist: {}",
                workspace_root.display()
            )));
        }

        let worktree_path = build_worktree_path(workspace_root, base_branch);
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| CoreError::Git(error.to_string()))?;
        }

        if is_git_repo(workspace_root) {
            run_git_stdout(
                ["worktree", "prune"],
                Some(workspace_root),
                GIT_LOCAL_TIMEOUT,
                "git worktree prune failed",
            )?;
            let worktree_add_args = vec![
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("--detach"),
                worktree_path.as_os_str().to_os_string(),
                OsString::from(base_branch),
            ];
            run_git_stdout(
                worktree_add_args,
                Some(workspace_root),
                GIT_NETWORK_TIMEOUT,
                "git worktree add failed",
            )?;
        } else {
            std::fs::create_dir_all(&worktree_path)
                .map_err(|error| CoreError::Git(error.to_string()))?;
        }

        Ok(PreparedWorktree {
            path: worktree_path.to_string_lossy().to_string(),
            branch: base_branch.to_string(),
            created_at: Utc::now().to_rfc3339(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    use super::{
        detect_workspace_setup_suggestions, is_broken_worktree_error_text,
        parse_default_branch_from_ls_remote_output, parse_local_branch_names,
    };

    #[test]
    fn parse_default_branch_from_ls_remote_output_extracts_branch_name() {
        let output = "ref: refs/heads/main\tHEAD\nabc123\tHEAD\n";

        assert_eq!(
            parse_default_branch_from_ls_remote_output(output),
            Some("main".to_string()),
        );
    }

    #[test]
    fn parse_default_branch_from_ls_remote_output_returns_none_when_missing() {
        let output = "abc123\trefs/heads/main\n";

        assert_eq!(parse_default_branch_from_ls_remote_output(output), None);
    }

    #[test]
    fn parse_local_branch_names_sorts_and_deduplicates() {
        let output = "feature/a\nmain\nfeature/a\nrelease\n";

        assert_eq!(
            parse_local_branch_names(output),
            vec![
                "feature/a".to_string(),
                "main".to_string(),
                "release".to_string(),
            ]
        );
    }

    #[test]
    fn detect_workspace_setup_suggestions_prefers_lockfile_package_manager() {
        let root = temp_test_dir("setup-node");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("package.json"), "{}").expect("write package.json");
        fs::write(root.join("yarn.lock"), "").expect("write yarn.lock");

        let suggestions = detect_workspace_setup_suggestions(
            root.to_str().expect("temp path should be valid utf-8"),
        );

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "yarn install");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detect_workspace_setup_suggestions_includes_rust_build() {
        let root = temp_test_dir("setup-rust");
        fs::create_dir_all(&root).expect("create temp dir");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write cargo");

        let suggestions = detect_workspace_setup_suggestions(
            root.to_str().expect("temp path should be valid utf-8"),
        );

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].command, "cargo build");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn broken_worktree_detection_matches_known_git_failures() {
        assert!(is_broken_worktree_error_text(
            "fatal: not a git repository: /tmp/repo/.git/worktrees/demo"
        ));
        assert!(is_broken_worktree_error_text(
            "fatal: cannot change to '/tmp/missing': No such file or directory"
        ));
        assert!(!is_broken_worktree_error_text(
            "fatal: not a valid object name HEAD~99"
        ));
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dcc-git-tests-{label}-{}", Uuid::new_v4()))
    }
}
