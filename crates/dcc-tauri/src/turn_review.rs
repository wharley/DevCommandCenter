use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use dcc_core::domain::session::{TurnReviewFile, TurnReviewUntrackedFingerprint};
use dcc_infra::git::{
    configure_git_command, parse_name_status_z, parse_numstat_z, GIT_LOCAL_TIMEOUT,
};
use uuid::Uuid;

pub const TURN_REVIEW_CAPTURE_VERSION: u32 = 1;
const MAX_DIFF_BYTES: usize = 512 * 1024;
const MAX_UNTRACKED_PATHS: usize = 1_000;
const MAX_CHANGED_FILES: usize = 5_000;
const MAX_PREVIEW_FILES: usize = 128;
const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;
const MAX_DIFF_RENDER_TIME: Duration = Duration::from_secs(5);
const MAX_RESULT_CAPTURE_TIME: Duration = Duration::from_secs(15);

thread_local! {
    static CAPTURE_DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

struct CaptureDeadlineGuard(Option<Instant>);

impl CaptureDeadlineGuard {
    fn start(duration: Duration) -> Self {
        let deadline = Instant::now() + duration;
        let previous = CAPTURE_DEADLINE.with(|active| active.replace(Some(deadline)));
        Self(previous)
    }
}

impl Drop for CaptureDeadlineGuard {
    fn drop(&mut self) {
        CAPTURE_DEADLINE.with(|active| active.set(self.0));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitTurnBaseline {
    pub tree: String,
    pub untracked: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct GitTurnResult {
    pub tree: String,
    pub status: String,
    pub files: Vec<TurnReviewFile>,
    pub file_diffs: BTreeMap<String, String>,
    pub diff_truncated: bool,
    pub result_untracked: Vec<TurnReviewUntrackedFingerprint>,
    pub excluded_preexisting_untracked: Vec<String>,
}

struct BoundedGitOutput {
    stdout: Vec<u8>,
    status: ExitStatus,
    stdout_truncated: bool,
}

fn kill_git(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    exceeded: Arc<AtomicBool>,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(8 * 1024));
    let mut buffer = [0u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(output);
        }
        let remaining = limit.saturating_sub(output.len());
        let accepted = remaining.min(read);
        output.extend_from_slice(&buffer[..accepted]);
        if accepted < read {
            exceeded.store(true, Ordering::Release);
        }
    }
}

fn run_git_bounded(
    root: &str,
    args: &[&str],
    envs: &[(OsString, OsString)],
    stdout_limit: usize,
    timeout: Duration,
    fallback: &str,
) -> Result<BoundedGitOutput, String> {
    let timeout = CAPTURE_DEADLINE.with(|active| match active.get() {
        Some(deadline) => deadline
            .checked_duration_since(Instant::now())
            .map(|remaining| remaining.min(timeout)),
        None => Some(timeout),
    });
    let Some(timeout) = timeout.filter(|duration| !duration.is_zero()) else {
        return Err(format!("{fallback}: turn review capture deadline exceeded"));
    };
    let mut command = Command::new("git");
    configure_git_command(&mut command);
    command
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.env_remove("GIT_CONFIG_PARAMETERS");
    command.env_remove("GIT_CONFIG_COUNT");
    for (key, _) in std::env::vars_os() {
        let key_text = key.to_string_lossy();
        if key_text.starts_with("GIT_CONFIG_KEY_") || key_text.starts_with("GIT_CONFIG_VALUE_") {
            command.env_remove(key);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture Git stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture Git stderr".to_string())?;
    let stdout_exceeded = Arc::new(AtomicBool::new(false));
    let stderr_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_reader = {
        let exceeded = stdout_exceeded.clone();
        thread::spawn(move || read_bounded(stdout, stdout_limit, exceeded))
    };
    let stderr_reader = {
        let exceeded = stderr_exceeded.clone();
        thread::spawn(move || read_bounded(stderr, MAX_STDERR_BYTES, exceeded))
    };
    let started = Instant::now();
    let status = loop {
        if stdout_exceeded.load(Ordering::Acquire) || stderr_exceeded.load(Ordering::Acquire) {
            kill_git(&mut child);
            break child.wait().map_err(|error| error.to_string())?;
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= timeout {
            kill_git(&mut child);
            let _ = child.wait();
            return Err(format!(
                "{fallback}: Git command timed out after {}s",
                timeout.as_secs_f32()
            ));
        }
        thread::sleep(Duration::from_millis(2));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Git stdout reader crashed".to_string())?
        .map_err(|error| error.to_string())?;
    let _stderr = stderr_reader
        .join()
        .map_err(|_| "Git stderr reader crashed".to_string())?
        .map_err(|error| error.to_string())?;
    if stderr_exceeded.load(Ordering::Acquire) {
        return Err(format!("{fallback}: Git stderr exceeded the safety limit"));
    }
    let stdout_truncated = stdout_exceeded.load(Ordering::Acquire);
    Ok(BoundedGitOutput {
        stdout,
        status,
        stdout_truncated,
    })
}

fn checked_output_limited(
    root: &str,
    args: &[&str],
    envs: &[(OsString, OsString)],
    limit: usize,
    fallback: &str,
) -> Result<Vec<u8>, String> {
    let output = run_git_bounded(root, args, envs, limit, GIT_LOCAL_TIMEOUT, fallback)?;
    if output.stdout_truncated {
        return Err(format!("{fallback}: output exceeded the safety limit"));
    }
    if !output.status.success() {
        return Err(fallback.to_string());
    }
    Ok(output.stdout)
}

fn list_untracked(root: &str) -> Result<Vec<String>, String> {
    let stdout = checked_output_limited(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &[],
        MAX_MANIFEST_BYTES,
        "failed to list untracked files",
    )?;
    let mut paths = stdout
        .split(|byte| *byte == b'\0')
        .filter(|field| !field.is_empty())
        .take(MAX_UNTRACKED_PATHS + 1)
        .map(|field| String::from_utf8_lossy(field).to_string())
        .collect::<Vec<_>>();
    if paths.len() > MAX_UNTRACKED_PATHS {
        return Err(format!(
            "untracked file limit exceeded ({MAX_UNTRACKED_PATHS}); review capture is unavailable"
        ));
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

struct TemporaryIndex {
    path: PathBuf,
}

impl Drop for TemporaryIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let lock_path = PathBuf::from(format!("{}.lock", self.path.display()));
        let _ = fs::remove_file(lock_path);
    }
}

fn quarantine_env(root: &str, snapshot_root: &Path) -> Result<Vec<(OsString, OsString)>, String> {
    let common_dir = String::from_utf8_lossy(&checked_output_limited(
        root,
        &["rev-parse", "--git-common-dir"],
        &[],
        4 * 1024,
        "failed to resolve Git common directory",
    )?)
    .trim()
    .to_string();
    let common_dir = if Path::new(&common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        Path::new(root).join(common_dir)
    };
    let real_objects = common_dir
        .join("objects")
        .canonicalize()
        .map_err(|_| "failed to resolve Git object directory".to_string())?;
    let quarantine_objects = snapshot_root.join("objects");
    let shadow_git_dir = snapshot_root.join("git-dir");
    let empty_global_config = snapshot_root.join("empty-global-config");
    fs::create_dir_all(&quarantine_objects).map_err(|error| error.to_string())?;
    fs::create_dir_all(&shadow_git_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(shadow_git_dir.join("objects")).map_err(|error| error.to_string())?;
    fs::create_dir_all(shadow_git_dir.join("refs").join("heads"))
        .map_err(|error| error.to_string())?;
    fs::write(shadow_git_dir.join("HEAD"), b"ref: refs/heads/dcc-shadow\n")
        .map_err(|error| error.to_string())?;
    fs::write(
        shadow_git_dir.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(&empty_global_config, b"").map_err(|error| error.to_string())?;
    Ok(vec![
        (OsString::from("GIT_DIR"), shadow_git_dir.into_os_string()),
        (
            OsString::from("GIT_WORK_TREE"),
            Path::new(root).as_os_str().to_os_string(),
        ),
        (
            OsString::from("GIT_OBJECT_DIRECTORY"),
            quarantine_objects.into_os_string(),
        ),
        (
            OsString::from("GIT_ALTERNATE_OBJECT_DIRECTORIES"),
            real_objects.into_os_string(),
        ),
        (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
        (
            OsString::from("GIT_CONFIG_GLOBAL"),
            empty_global_config.into_os_string(),
        ),
    ])
}

fn with_index_env(mut envs: Vec<(OsString, OsString)>, index: &Path) -> Vec<(OsString, OsString)> {
    envs.push((
        OsString::from("GIT_INDEX_FILE"),
        index.as_os_str().to_os_string(),
    ));
    envs
}

fn capture_tracked_tree(root: &str, snapshot_root: &Path) -> Result<String, String> {
    fs::create_dir_all(snapshot_root).map_err(|error| error.to_string())?;
    let index = TemporaryIndex {
        path: snapshot_root.join(format!("turn-review-{}.index", Uuid::new_v4())),
    };

    let index_path = String::from_utf8_lossy(&checked_output_limited(
        root,
        &["rev-parse", "--git-path", "index"],
        &[],
        4 * 1024,
        "failed to resolve Git index",
    )?)
    .trim()
    .to_string();
    let real_index = if Path::new(&index_path).is_absolute() {
        PathBuf::from(index_path)
    } else {
        Path::new(root).join(index_path)
    };
    if real_index.is_file() {
        fs::copy(&real_index, &index.path).map_err(|error| error.to_string())?;
    } else {
        let envs = with_index_env(quarantine_env(root, snapshot_root)?, &index.path);
        checked_output_limited(
            root,
            &["read-tree", "--empty"],
            &envs,
            8 * 1024,
            "failed to initialize temporary Git index",
        )?;
    }

    let envs = with_index_env(quarantine_env(root, snapshot_root)?, &index.path);
    checked_output_limited(
        root,
        &["add", "-u", "--", "."],
        &envs,
        8 * 1024,
        "failed to snapshot tracked worktree files",
    )?;
    let tree = checked_output_limited(
        root,
        &["write-tree"],
        &envs,
        256,
        "failed to write turn review tree",
    )?;
    let tree = String::from_utf8_lossy(&tree).trim().to_string();
    if tree.len() != 40 && tree.len() != 64 {
        return Err("Git returned an invalid turn review tree fingerprint".to_string());
    }
    Ok(tree)
}

pub fn capture_baseline(root: &str, temporary_root: &Path) -> Result<GitTurnBaseline, String> {
    let root_path = Path::new(root);
    if root.trim().is_empty() || !root_path.is_absolute() || !root_path.is_dir() {
        return Err("workspace path is unavailable for turn review".to_string());
    }
    Ok(GitTurnBaseline {
        tree: capture_tracked_tree(root, temporary_root)?,
        untracked: list_untracked(root)?,
    })
}

pub fn cleanup_snapshot(snapshot_root: &Path) {
    let is_snapshot = snapshot_root
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(|name| Uuid::parse_str(name).ok())
        .is_some();
    if is_snapshot {
        let _ = fs::remove_dir_all(snapshot_root);
    }
}

pub fn cleanup_all_snapshot_quarantines(snapshots_root: &Path) {
    let Ok(entries) = fs::read_dir(snapshots_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && path
                .file_name()
                .and_then(OsStr::to_str)
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_some()
        {
            cleanup_snapshot(&path);
        }
    }
}

fn binary_paths_from_numstat(stdout: &[u8]) -> BTreeSet<String> {
    let mut binary = BTreeSet::new();
    let mut cursor = 0usize;
    while cursor < stdout.len() {
        let Some(ins_end_rel) = stdout[cursor..].iter().position(|byte| *byte == b'\t') else {
            break;
        };
        let ins_end = cursor + ins_end_rel;
        let Some(del_end_rel) = stdout[ins_end + 1..].iter().position(|byte| *byte == b'\t') else {
            break;
        };
        let del_end = ins_end + 1 + del_end_rel;
        let Some(path_end_rel) = stdout[del_end + 1..].iter().position(|byte| *byte == b'\0')
        else {
            break;
        };
        let path_end = del_end + 1 + path_end_rel;
        let is_binary = &stdout[cursor..ins_end] == b"-" && &stdout[ins_end + 1..del_end] == b"-";
        let path_bytes = &stdout[del_end + 1..path_end];
        let path = if path_bytes.is_empty() {
            let old_start = path_end + 1;
            let Some(old_end_rel) = stdout[old_start..].iter().position(|byte| *byte == b'\0')
            else {
                break;
            };
            let old_end = old_start + old_end_rel;
            let new_start = old_end + 1;
            let Some(new_end_rel) = stdout[new_start..].iter().position(|byte| *byte == b'\0')
            else {
                break;
            };
            let new_end = new_start + new_end_rel;
            cursor = new_end + 1;
            String::from_utf8_lossy(&stdout[new_start..new_end]).to_string()
        } else {
            cursor = path_end + 1;
            String::from_utf8_lossy(path_bytes).to_string()
        };
        if is_binary && !path.is_empty() {
            binary.insert(path);
        }
    }
    binary
}

pub fn capture_result(
    root: &str,
    snapshot_root: &Path,
    baseline: &GitTurnBaseline,
) -> Result<GitTurnResult, String> {
    let _capture_deadline = CaptureDeadlineGuard::start(MAX_RESULT_CAPTURE_TIME);
    let result_tree = capture_tracked_tree(root, snapshot_root)?;
    let current_untracked = list_untracked(root)?;
    let baseline_untracked = baseline.untracked.iter().cloned().collect::<BTreeSet<_>>();
    let current_untracked_set = current_untracked.iter().cloned().collect::<BTreeSet<_>>();
    let new_untracked = current_untracked_set
        .difference(&baseline_untracked)
        .cloned()
        .collect::<Vec<_>>();
    let excluded_preexisting_untracked = current_untracked_set
        .intersection(&baseline_untracked)
        .cloned()
        .collect::<Vec<_>>();

    let object_env = quarantine_env(root, snapshot_root)?;
    let name_status = run_git_bounded(
        root,
        &["diff", "--name-status", "-z", &baseline.tree, &result_tree],
        &object_env,
        MAX_MANIFEST_BYTES,
        GIT_LOCAL_TIMEOUT,
        "failed to list turn changes",
    )?;
    if !name_status.status.success() && !name_status.stdout_truncated {
        return Err("failed to list turn changes".to_string());
    }
    let numstat = run_git_bounded(
        root,
        &["diff", "--numstat", "-z", &baseline.tree, &result_tree],
        &object_env,
        MAX_MANIFEST_BYTES,
        GIT_LOCAL_TIMEOUT,
        "failed to summarize turn changes",
    )?;
    if !numstat.status.success() && !numstat.stdout_truncated {
        return Err("failed to summarize turn changes".to_string());
    }
    let mut manifest_partial = name_status.stdout_truncated || numstat.stdout_truncated;
    let stats = parse_numstat_z(&numstat.stdout);
    let binary_paths = binary_paths_from_numstat(&numstat.stdout);
    let parsed_files = parse_name_status_z(&name_status.stdout);
    if parsed_files.len() > MAX_CHANGED_FILES {
        manifest_partial = true;
    }
    let mut files = parsed_files
        .into_iter()
        .take(MAX_CHANGED_FILES)
        .map(|entry| {
            let (insertions, deletions) = stats.get(&entry.path).copied().unwrap_or_default();
            TurnReviewFile {
                path: entry.path,
                old_path: entry.old_path,
                status: entry.status,
                insertions,
                deletions,
                untracked: false,
                binary: false,
                preview_unavailable: false,
            }
        })
        .collect::<Vec<_>>();
    for file in &mut files {
        file.binary = binary_paths.contains(&file.path);
        file.preview_unavailable = file.binary;
    }

    let mut file_diffs = BTreeMap::new();
    let mut remaining_diff_budget = MAX_DIFF_BYTES;
    let mut diff_truncated = manifest_partial;
    let diff_deadline = Instant::now() + MAX_DIFF_RENDER_TIME;
    for (index, file) in files.iter_mut().enumerate() {
        if file.binary {
            continue;
        }
        let remaining_time = diff_deadline.saturating_duration_since(Instant::now());
        if index >= MAX_PREVIEW_FILES || remaining_diff_budget == 0 || remaining_time.is_zero() {
            file.preview_unavailable = true;
            diff_truncated = true;
            continue;
        }
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--unified=3",
            &baseline.tree,
            &result_tree,
            "--",
        ];
        if let Some(old_path) = file.old_path.as_deref() {
            args.push(old_path);
        }
        args.push(&file.path);
        let rendered = match run_git_bounded(
            root,
            &args,
            &object_env,
            remaining_diff_budget,
            remaining_time.min(GIT_LOCAL_TIMEOUT),
            "failed to render turn file diff",
        ) {
            Ok(rendered) => rendered,
            Err(_) => {
                file.preview_unavailable = true;
                diff_truncated = true;
                remaining_diff_budget = 0;
                continue;
            }
        };
        if !rendered.status.success() && !rendered.stdout_truncated {
            file.preview_unavailable = true;
            diff_truncated = true;
            continue;
        }
        if rendered.stdout_truncated {
            file.preview_unavailable = true;
            diff_truncated = true;
            remaining_diff_budget = 0;
        } else if !rendered.stdout.is_empty() {
            remaining_diff_budget = remaining_diff_budget.saturating_sub(rendered.stdout.len());
            file_diffs.insert(
                file.path.clone(),
                String::from_utf8_lossy(&rendered.stdout).to_string(),
            );
        }
    }
    // V1 deliberately never reads untracked content. Cross-platform safe
    // open-by-handle semantics are not available here, so these paths remain
    // visible but cannot be previewed or used as compatibility evidence.
    for path in &new_untracked {
        files.push(TurnReviewFile {
            path: path.clone(),
            old_path: None,
            status: "A".to_string(),
            insertions: 0,
            deletions: 0,
            untracked: true,
            binary: false,
            preview_unavailable: true,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let has_changes = !files.is_empty() || !new_untracked.is_empty();
    let status = if !has_changes {
        "no_changes"
    } else if diff_truncated
        || !new_untracked.is_empty()
        || !excluded_preexisting_untracked.is_empty()
    {
        "partial"
    } else {
        "available"
    };
    Ok(GitTurnResult {
        tree: result_tree,
        status: status.to_string(),
        files,
        file_diffs,
        diff_truncated,
        result_untracked: Vec::new(),
        excluded_preexisting_untracked,
    })
}

pub fn current_tree_matches(
    root: &str,
    snapshot_root: &Path,
    expected_tree: &str,
) -> Result<bool, String> {
    let result = capture_tracked_tree(root, snapshot_root).map(|tree| tree == expected_tree);
    cleanup_snapshot(snapshot_root);
    result
}

pub fn current_snapshot_matches(
    root: &str,
    temporary_root: &Path,
    expected_tree: &str,
    baseline_untracked: &[String],
    result_untracked: &[TurnReviewUntrackedFingerprint],
) -> Result<bool, String> {
    if !current_tree_matches(root, temporary_root, expected_tree)? {
        return Ok(false);
    }
    let current = list_untracked(root)?.into_iter().collect::<BTreeSet<_>>();
    let expected = baseline_untracked
        .iter()
        .cloned()
        .chain(result_untracked.iter().map(|entry| entry.path.clone()))
        .collect::<BTreeSet<_>>();
    if current != expected {
        return Ok(false);
    }
    Ok(true)
}

pub fn observed_validations_for_turn(commands: impl IntoIterator<Item = String>) -> Vec<String> {
    const TOKENS: [&str; 9] = [
        " test",
        "test ",
        "typecheck",
        " check",
        "check ",
        " lint",
        "lint ",
        " build",
        "build ",
    ];
    let mut seen = BTreeSet::new();
    commands
        .into_iter()
        .filter_map(|command| {
            let trimmed = command.trim();
            let lower = format!(" {trimmed} ").to_lowercase();
            if !trimmed.is_empty() && lower.contains("typecheck") {
                Some("typecheck".to_string())
            } else if !trimmed.is_empty() && (lower.contains(" test ") || lower.contains("test ")) {
                Some("test".to_string())
            } else if !trimmed.is_empty() && (lower.contains(" lint ") || lower.contains("lint ")) {
                Some("lint".to_string())
            } else if !trimmed.is_empty() && (lower.contains(" build ") || lower.contains("build "))
            {
                Some("build".to_string())
            } else if !trimmed.is_empty() && TOKENS.iter().any(|token| lower.contains(token)) {
                Some("check".to_string())
            } else {
                None
            }
        })
        .filter(|command| seen.insert(command.clone()))
        .take(20)
        .collect()
}

pub fn file_totals(files: &[TurnReviewFile]) -> (u32, u32) {
    files.iter().fold((0, 0), |(insertions, deletions), file| {
        (
            insertions.saturating_add(file.insertions),
            deletions.saturating_add(file.deletions),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn snapshot_root(container: &Path) -> PathBuf {
        container.join(Uuid::new_v4().to_string())
    }

    fn object_files(root: &Path) -> BTreeSet<PathBuf> {
        fn visit(root: &Path, current: &Path, result: &mut BTreeSet<PathBuf>) {
            let Ok(entries) = fs::read_dir(current) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, result);
                } else if let Ok(relative) = path.strip_prefix(root) {
                    result.insert(relative.to_path_buf());
                }
            }
        }
        let objects = root.join(".git").join("objects");
        let mut result = BTreeSet::new();
        visit(&objects, &objects, &mut result);
        result
    }

    #[test]
    fn snapshot_excludes_preexisting_changes_and_preserves_real_index() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        git(root, &["config", "user.email", "dcc@example.invalid"]);
        git(root, &["config", "user.name", "DCC Test"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["add", "tracked.txt"]);
        git(root, &["commit", "-m", "base"]);
        fs::write(root.join("tracked.txt"), "before turn\n").unwrap();
        fs::write(root.join("private.local"), "before\n").unwrap();
        let index_before = checked_output_limited(
            root.to_str().unwrap(),
            &["ls-files", "--stage"],
            &[],
            64 * 1024,
            "index",
        )
        .unwrap();
        let objects_before = object_files(root);
        let snapshots = tempfile::tempdir().unwrap();
        let snapshot = snapshot_root(snapshots.path());

        let baseline = capture_baseline(root.to_str().unwrap(), &snapshot).unwrap();
        fs::write(root.join("tracked.txt"), "after turn\n").unwrap();
        fs::write(root.join("private.local"), "changed concurrently\n").unwrap();
        fs::write(root.join("created.txt"), "new\n").unwrap();
        let result = capture_result(root.to_str().unwrap(), &snapshot, &baseline).unwrap();

        let tracked = result.file_diffs.get("tracked.txt").unwrap();
        assert!(tracked.contains("-before turn"));
        assert!(tracked.contains("+after turn"));
        let created = result
            .files
            .iter()
            .find(|file| file.path == "created.txt")
            .unwrap();
        assert!(created.preview_unavailable);
        assert!(!result.file_diffs.contains_key("created.txt"));
        assert_eq!(result.status, "partial");
        assert_eq!(result.excluded_preexisting_untracked, vec!["private.local"]);
        let index_after = checked_output_limited(
            root.to_str().unwrap(),
            &["ls-files", "--stage"],
            &[],
            64 * 1024,
            "index",
        )
        .unwrap();
        assert_eq!(index_before, index_after);
        assert_eq!(objects_before, object_files(root));
        cleanup_snapshot(&snapshot);
        assert!(!snapshot.exists());
    }

    #[test]
    fn reports_no_changes_and_detects_later_workspace_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["add", "tracked.txt"]);
        let snapshots = tempfile::tempdir().unwrap();
        let snapshot = snapshot_root(snapshots.path());
        let baseline = capture_baseline(root.to_str().unwrap(), &snapshot).unwrap();
        let result = capture_result(root.to_str().unwrap(), &snapshot, &baseline).unwrap();
        assert_eq!(result.status, "no_changes");
        let compatibility = snapshot_root(snapshots.path());
        assert!(
            current_tree_matches(root.to_str().unwrap(), &compatibility, &result.tree).unwrap()
        );
        fs::write(root.join("tracked.txt"), "later\n").unwrap();
        let compatibility = snapshot_root(snapshots.path());
        assert!(
            !current_tree_matches(root.to_str().unwrap(), &compatibility, &result.tree).unwrap()
        );
        cleanup_snapshot(&snapshot);
    }

    #[test]
    fn oversized_diff_is_killed_and_not_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["add", "tracked.txt"]);
        let snapshots = tempfile::tempdir().unwrap();
        let snapshot = snapshot_root(snapshots.path());
        let baseline = capture_baseline(root.to_str().unwrap(), &snapshot).unwrap();
        fs::write(root.join("tracked.txt"), "changed\n".repeat(100_000)).unwrap();
        let result = capture_result(root.to_str().unwrap(), &snapshot, &baseline).unwrap();
        assert_eq!(result.status, "partial");
        assert!(result.diff_truncated);
        assert!(result.file_diffs.is_empty());
        assert!(result.files[0].preview_unavailable);
        cleanup_snapshot(&snapshot);
    }

    #[cfg(unix)]
    #[test]
    fn shadow_git_dir_never_executes_repo_clean_filter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init"]);
        fs::write(root.join(".gitattributes"), "*.txt filter=dcc-review\n").unwrap();
        fs::write(root.join("tracked.txt"), "base\n").unwrap();
        git(root, &["add", ".gitattributes", "tracked.txt"]);
        let sentinel = root.join("filter-ran");
        let driver = format!("sh -c 'touch \"{}\"; cat'", sentinel.display());
        git(
            root,
            &["config", "filter.dcc-review.clean", driver.as_str()],
        );
        fs::write(root.join("tracked.txt"), "changed\n").unwrap();
        let snapshots = tempfile::tempdir().unwrap();
        let snapshot = snapshot_root(snapshots.path());
        let previous_parameters = std::env::var_os("GIT_CONFIG_PARAMETERS");
        unsafe {
            std::env::set_var(
                "GIT_CONFIG_PARAMETERS",
                "'filter.dcc-review.clean=false' 'filter.dcc-review.required=true'",
            );
        }
        capture_baseline(root.to_str().unwrap(), &snapshot).unwrap();
        unsafe {
            match previous_parameters {
                Some(value) => std::env::set_var("GIT_CONFIG_PARAMETERS", value),
                None => std::env::remove_var("GIT_CONFIG_PARAMETERS"),
            }
        }
        assert!(!sentinel.exists());
        cleanup_snapshot(&snapshot);
    }

    #[test]
    fn startup_sweep_removes_only_uuid_snapshot_directories() {
        let container = tempfile::tempdir().unwrap();
        let orphan = snapshot_root(container.path());
        let preserved = container.path().join("do-not-remove");
        fs::create_dir_all(&orphan).unwrap();
        fs::create_dir_all(&preserved).unwrap();
        fs::write(orphan.join("secret-object"), b"secret").unwrap();
        cleanup_all_snapshot_quarantines(container.path());
        assert!(!orphan.exists());
        assert!(preserved.exists());
    }

    #[test]
    fn validation_evidence_is_bounded_and_deduplicated() {
        let commands = vec![
            "yarn test".to_string(),
            "yarn test".to_string(),
            "git status".to_string(),
        ];
        assert_eq!(observed_validations_for_turn(commands), vec!["test"]);
    }
}
