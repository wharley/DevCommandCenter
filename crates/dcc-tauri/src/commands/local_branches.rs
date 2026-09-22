//! Branch selection for a local-direct task. Conversation starts and checkout
//! share a physical-root lock, so a stale renderer cannot switch a started task.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};

use dcc_core::{
    domain::{
        session::SessionId,
        workspace::{Workspace, WorkspaceId},
    },
    ports::{SessionRepo, WorkspaceRepo},
};
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::workspace_support::run_git_network_output_with_workspace_auth;
use crate::{
    git::{git_output_err, run_git_output},
    state::WorkspaceCommandState,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalBranchesInput {
    pub workspace_id: WorkspaceId,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalBranchEntry {
    pub name: String,
    pub reference: String,
    pub remote: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalBranchesOutput {
    pub current_branch: Option<String>,
    pub task_branch: String,
    pub has_conversation: bool,
    pub agent_running: bool,
    pub branches: Vec<LocalBranchEntry>,
    pub refresh_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SwitchLocalBranchInput {
    pub workspace_id: WorkspaceId,
    pub expected_branch: Option<String>,
    /// Full ref returned by the listing. None when creating from HEAD.
    pub reference: Option<String>,
    pub new_branch: Option<String>,
}

type RootLocks = BTreeMap<PathBuf, Weak<AsyncMutex<()>>>;
static ROOT_LOCKS: OnceLock<Mutex<RootLocks>> = OnceLock::new();

async fn lock_root(root: &str) -> Result<OwnedMutexGuard<()>, String> {
    let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let lock = {
        let mut locks = ROOT_LOCKS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|e| e.to_string())?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        let lock = locks
            .get(&root)
            .and_then(Weak::upgrade)
            .unwrap_or_else(|| Arc::new(AsyncMutex::new(())));
        locks.insert(root, Arc::downgrade(&lock));
        lock
    };
    Ok(lock.lock_owned().await)
}

fn git(root: &str, args: &[&str]) -> Result<String, String> {
    let output = run_git_output(root, args)?;
    if !output.status.success() {
        return Err(git_output_err("git", &output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

pub(crate) fn current_branch(root: &str) -> Result<Option<String>, String> {
    let output = run_git_output(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        Err(git_output_err("git symbolic-ref", &output.stderr))
    }
}

async fn local_workspace(db: &Path, id: &WorkspaceId) -> Result<Workspace, String> {
    let repo = SqliteWorkspaceRepo::open(db).map_err(|e| e.to_string())?;
    let workspace = repo
        .get_workspace(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Task not found")?;
    if workspace.worktree_path.is_some() {
        return Err("Branch selection is only available for local-direct tasks".into());
    }
    Ok(workspace)
}

// Includes archived conversations and sessions that authorize this task as an
// additional root; neither closing a thread nor changing the selected tab unlocks it.
fn session_ids(db: &Path, workspace_id: &WorkspaceId) -> Result<Vec<SessionId>, String> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT id FROM dcc_sessions WHERE workspace_id = ?1 OR EXISTS (SELECT 1 FROM json_each(additional_workspace_ids_json) WHERE value = ?1)").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([&workspace_id.0], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| r.map(SessionId).map_err(|e| e.to_string()))
        .collect()
}

async fn conversation_state(db: &Path, id: &WorkspaceId) -> Result<(bool, bool), String> {
    use rusqlite::OptionalExtension;
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let mut started = false;
    let mut running = false;
    for session in session_ids(db, id)? {
        // Query only turn boundaries: never load assistant/tool payloads just
        // to render the branch indicator on a long conversation.
        started |= conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM dcc_session_events WHERE session_id = ?1 AND json_extract(kind_json, '$.type') IN ('turn_started', 'turn_queued', 'turn_steered'))",
            [&session.0], |row| row.get::<_, bool>(0),
        ).map_err(|e| e.to_string())?;
        let last: Option<(String, i64)> = conn.query_row(
            "SELECT json_extract(kind_json, '$.turnId'), sequence FROM dcc_session_events WHERE session_id = ?1 AND json_extract(kind_json, '$.type') = 'turn_started' ORDER BY sequence DESC LIMIT 1",
            [&session.0], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(|e| e.to_string())?;
        if let Some((turn, sequence)) = last {
            let ended: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM dcc_session_events WHERE session_id = ?1 AND sequence > ?2 AND (json_extract(kind_json, '$.type') IN ('session_completed', 'session_aborted') OR (json_extract(kind_json, '$.type') IN ('turn_completed', 'turn_aborted') AND json_extract(kind_json, '$.turnId') = ?3)))",
                rusqlite::params![session.0, sequence, turn], |row| row.get(0),
            ).map_err(|e| e.to_string())?;
            running |= !ended;
        }
    }
    Ok((started, running))
}

async fn status(db: &Path, workspace: &Workspace) -> Result<LocalBranchesOutput, String> {
    let (has_conversation, mut agent_running) = conversation_state(db, &workspace.id).await?;
    let root = std::fs::canonicalize(&workspace.root_path).map_err(|e| e.to_string())?;
    let repo = SqliteWorkspaceRepo::open(db).map_err(|e| e.to_string())?;
    let current = current_branch(&workspace.root_path)?;
    let mut bound = repo
        .local_branch_binding(&workspace.id)
        .map_err(|e| e.to_string())?;
    if has_conversation && bound.is_none() {
        // Older releases stored the project's default as base_branch even when
        // local work was on another branch. Establish a baseline on first use
        // after upgrading; historical checkout cannot be inferred safely.
        if let Some(branch) = current.as_deref() {
            repo.bind_local_branch(&workspace.id, branch)
                .map_err(|e| e.to_string())?;
            bound = Some(branch.to_string());
        }
    }
    let has_conversation = has_conversation || bound.is_some();
    let task_branch = bound
        .or_else(|| current.clone())
        .unwrap_or_else(|| workspace.base_branch.clone());
    for other in repo.list_workspaces().await.map_err(|e| e.to_string())? {
        if other.id != workspace.id
            && other.worktree_path.is_none()
            && std::fs::canonicalize(&other.root_path).ok().as_ref() == Some(&root)
        {
            agent_running |= conversation_state(db, &other.id).await?.1;
        }
    }
    let branches = git(
        &workspace.root_path,
        &[
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname)%09%(symref)",
            "refs/heads/",
            "refs/remotes/",
        ],
    )?
    .lines()
    .filter_map(|line| {
        let (reference, symbolic) = line.split_once('\t').unwrap_or((line, ""));
        if !symbolic.is_empty() {
            return None;
        }
        let (name, remote) = if let Some(name) = reference.strip_prefix("refs/heads/") {
            (name, false)
        } else {
            (reference.strip_prefix("refs/remotes/")?, true)
        };
        Some(LocalBranchEntry {
            name: name.into(),
            reference: reference.into(),
            remote,
        })
    })
    .collect();
    Ok(LocalBranchesOutput {
        current_branch: current,
        task_branch,
        has_conversation,
        agent_running,
        branches,
        refresh_error: None,
    })
}

#[tauri::command]
pub async fn workspace_local_branches(
    state: State<'_, WorkspaceCommandState>,
    input: LocalBranchesInput,
) -> Result<LocalBranchesOutput, String> {
    let workspace = local_workspace(&state.db_path, &input.workspace_id).await?;
    let db = state.db_path.clone();
    // Fetch never holds the conversation/checkout lock while waiting on network.
    let refresh_error = if input.refresh {
        let root = workspace.root_path.clone();
        let db = db.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let mut errors = Vec::new();
            for remote in git(&root, &["remote"])?.lines() {
                let result = run_git_network_output_with_workspace_auth(
                    &db,
                    &root,
                    &["fetch", "--prune", "--no-tags", remote],
                    None,
                );
                match result {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => errors.push(git_output_err("git fetch", &output.stderr)),
                    Err(error) => errors.push(error),
                }
            }
            Ok::<_, String>((!errors.is_empty()).then(|| errors.join("\n")))
        })
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|e| Some(e))
    } else {
        None
    };
    let _guard = lock_root(&workspace.root_path).await?;
    tauri::async_runtime::spawn_blocking(move || {
        futures::executor::block_on(async {
            let workspace = local_workspace(&db, &input.workspace_id).await?;
            let mut result = status(&db, &workspace).await?;
            result.refresh_error = refresh_error;
            Ok(result)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn switch_branch(
    db: &Path,
    input: &SwitchLocalBranchInput,
) -> Result<LocalBranchesOutput, String> {
    let mut workspace = local_workspace(db, &input.workspace_id).await?;
    let before = status(db, &workspace).await?;
    if before.has_conversation {
        return Err("The branch cannot be changed after the conversation has started.".into());
    }
    if before.agent_running {
        return Err(
            "An agent is working in this folder. Wait for it to finish before changing branches."
                .into(),
        );
    }
    if before.current_branch != input.expected_branch {
        return Err(
            "The active branch changed outside this task. Refresh the branch list and try again."
                .into(),
        );
    }
    let root = &workspace.root_path;
    match (&input.new_branch, &input.reference) {
        (Some(name), None) => {
            let name = name.trim();
            if name.starts_with('-') || name.is_empty() {
                return Err("Invalid branch name".into());
            }
            git(root, &["check-ref-format", &format!("refs/heads/{name}")])?;
            git(root, &["switch", "--no-guess", "-c", name])?;
        }
        (None, Some(reference)) => {
            let selected = before
                .branches
                .iter()
                .find(|branch| &branch.reference == reference)
                .ok_or("Branch no longer exists. Refresh the list.")?;
            if selected.remote {
                // Explicit full ref avoids ambiguity between remotes. Git derives
                // the local name and refuses collisions / worktree conflicts.
                let tracking = git(
                    root,
                    &[
                        "for-each-ref",
                        "--format=%(refname:short)%09%(upstream)",
                        "refs/heads/",
                    ],
                )?;
                if let Some(local) = tracking.lines().find_map(|line| {
                    line.split_once('\t')
                        .filter(|(_, upstream)| *upstream == reference)
                        .map(|(name, _)| name)
                }) {
                    git(root, &["switch", "--no-guess", "--", local])?;
                } else {
                    git(root, &["switch", "--track", reference])?;
                }
            } else {
                git(root, &["switch", "--no-guess", "--", &selected.name])?;
            }
        }
        _ => return Err("Choose an existing branch or provide a new branch name".into()),
    }
    workspace.base_branch = current_branch(root)?.ok_or("The repository has a detached HEAD")?;
    workspace.updated_at = chrono::Utc::now().to_rfc3339();
    SqliteWorkspaceRepo::open(db)
        .map_err(|e| e.to_string())?
        .save_workspace(&workspace)
        .await
        .map_err(|e| e.to_string())?;
    status(db, &workspace).await
}

#[tauri::command]
pub async fn workspace_switch_local_branch(
    state: State<'_, WorkspaceCommandState>,
    input: SwitchLocalBranchInput,
) -> Result<LocalBranchesOutput, String> {
    let workspace = local_workspace(&state.db_path, &input.workspace_id).await?;
    let guard = lock_root(&workspace.root_path).await?;
    let db = state.db_path.clone();
    state
        .run_git_workspace_mutation_for_workspace_blocking(
            &workspace.id,
            &workspace.root_path,
            move |_| {
                // Keep admission held even if the async command is cancelled
                // while its blocking Git operation is still running.
                let _guard = guard;
                futures::executor::block_on(switch_branch(&db, &input))
            },
        )
        .await
        .map_err(super::workspace_commands::workspace_mutation_error)
}

/// Called at the durable event boundary for all turn entry points (desktop,
/// queued turns, automations and remote API), while the root locks remain held.
pub(crate) async fn guard_conversation_event(
    db: &Path,
    session_id: &SessionId,
) -> Result<Vec<OwnedMutexGuard<()>>, String> {
    let sessions = SqliteSessionRepo::open_read_only(db).map_err(|e| e.to_string())?;
    let Some(session) = sessions
        .get_session(session_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(vec![]);
    };
    let repo = SqliteWorkspaceRepo::open(db).map_err(|e| e.to_string())?;
    let mut locals = BTreeMap::new();
    for id in std::iter::once(&session.workspace_id).chain(session.additional_workspace_ids.iter())
    {
        if let Some(workspace) = repo.get_workspace(id).await.map_err(|e| e.to_string())? {
            if workspace.worktree_path.is_none()
                && dcc_infra::git::is_git_repo(Path::new(&workspace.root_path))
            {
                let physical =
                    std::fs::canonicalize(&workspace.root_path).map_err(|e| e.to_string())?;
                locals
                    .entry(physical)
                    .or_insert_with(Vec::new)
                    .push(workspace.id);
            }
        }
    }
    let mut guards = Vec::new();
    for (root, ids) in locals {
        guards.push(lock_root(root.to_str().ok_or("Invalid repository path")?).await?);
        for id in ids {
            let mut workspace = local_workspace(db, &id).await?;
            let branch = current_branch(&workspace.root_path)?.ok_or(
                "Choose a branch before starting a local-direct conversation (detached HEAD).",
            )?;
            if let Some(expected) = repo.local_branch_binding(&id).map_err(|e| e.to_string())? {
                if expected != branch {
                    return Err(format!("This conversation belongs to branch `{expected}`, but the folder is now on `{branch}`. Restore `{expected}` in the terminal or start a new task before continuing."));
                }
            } else {
                // First user event binds the actual branch, including legacy
                // tasks whose base_branch was the project's configured default.
                workspace.base_branch = branch;
                workspace.updated_at = chrono::Utc::now().to_rfc3339();
                repo.save_workspace(&workspace)
                    .await
                    .map_err(|e| e.to_string())?;
                repo.bind_local_branch(&id, &workspace.base_branch)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(guards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SessionCommandState;
    use dcc_core::domain::{
        project::ProjectId,
        session::{Session, SessionEventKind, SessionEventRecord, SessionState, TurnId},
        workspace::WorkspaceState,
    };
    use dcc_core::ports::SessionEventRepo;

    struct Fixture {
        _dir: tempfile::TempDir,
        db: PathBuf,
        root: String,
        workspace: Workspace,
        sessions: SessionCommandState,
    }
    impl Fixture {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("repo");
            std::fs::create_dir(&root).unwrap();
            let root = root.to_str().unwrap().to_string();
            git(&root, &["init", "-b", "main"]).unwrap();
            git(
                &root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "initial",
                ],
            )
            .unwrap();
            let physical = std::fs::canonicalize(dir.path()).unwrap();
            let db = physical.join("state.sqlite");
            let sessions = SessionCommandState::new_headless(db.clone(), physical.join("app"));
            let workspace = Workspace {
                id: WorkspaceId("local".into()),
                project_id: ProjectId("project".into()),
                name: None,
                root_path: root.clone(),
                base_branch: "main".into(),
                worktree_path: None,
                source: None,
                state: WorkspaceState::Ready,
                setup_report: None,
                pinned_at: None,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
            };
            SqliteWorkspaceRepo::open(&db)
                .unwrap()
                .save_workspace(&workspace)
                .await
                .unwrap();
            Self {
                _dir: dir,
                db,
                root,
                workspace,
                sessions,
            }
        }
        fn selection(&self, reference: Option<&str>, name: Option<&str>) -> SwitchLocalBranchInput {
            SwitchLocalBranchInput {
                workspace_id: self.workspace.id.clone(),
                expected_branch: Some("main".into()),
                reference: reference.map(Into::into),
                new_branch: name.map(Into::into),
            }
        }
        async fn session(&self, id: &str, workspace: &Workspace) -> SessionId {
            let id = SessionId(id.into());
            let session = Session {
                id: id.clone(),
                project_id: workspace.project_id.clone(),
                workspace_id: workspace.id.clone(),
                additional_workspace_ids: vec![],
                provider_id: "codex".into(),
                model: None,
                provider_runtime: None,
                working_directory_override: None,
                state: SessionState::Active,
                created_at: workspace.created_at.clone(),
                updated_at: workspace.updated_at.clone(),
            };
            self.sessions.save_session(&session).await.unwrap();
            self.event(
                &id,
                1,
                SessionEventKind::SessionStarted {
                    workspace_id: workspace.id.clone(),
                    project_id: workspace.project_id.clone(),
                    provider_id: "codex".into(),
                    model: None,
                    forked_from: None,
                },
            )
            .await
            .unwrap();
            id
        }
        async fn event(
            &self,
            session: &SessionId,
            sequence: u64,
            kind: SessionEventKind,
        ) -> Result<(), String> {
            self.sessions
                .append_event(&SessionEventRecord {
                    event_id: uuid::Uuid::new_v4().to_string(),
                    session_id: session.clone(),
                    sequence,
                    occurred_at: self.workspace.created_at.clone(),
                    kind,
                })
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        fn started() -> SessionEventKind {
            SessionEventKind::TurnStarted {
                turn_id: TurnId("turn".into()),
                prompt: "hello".into(),
                plan_mode: None,
                model: None,
                evidence: None,
                retry_of_turn_id: None,
            }
        }
    }

    #[test]
    fn creates_and_switches_in_place_without_a_worktree() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let result = switch_branch(&f.db, &f.selection(None, Some("feature/new")))
                .await
                .unwrap();
            assert_eq!(result.current_branch.as_deref(), Some("feature/new"));
            assert_eq!(result.task_branch, "feature/new");
            assert!(!result.has_conversation);
            assert_eq!(
                git(&f.root, &["worktree", "list", "--porcelain"])
                    .unwrap()
                    .matches("worktree ")
                    .count(),
                1
            );
            let mut input = f.selection(Some("refs/heads/main"), None);
            input.expected_branch = Some("feature/new".into());
            assert_eq!(
                switch_branch(&f.db, &input)
                    .await
                    .unwrap()
                    .current_branch
                    .as_deref(),
                Some("main")
            );
        });
    }

    #[test]
    fn rejects_stale_selection_invalid_names_and_protected_worktrees() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            for name in ["-force", "../bad", "main", "@{-1}"] {
                assert!(
                    switch_branch(&f.db, &f.selection(None, Some(name)))
                        .await
                        .is_err(),
                    "{name}"
                );
                assert_eq!(current_branch(&f.root).unwrap().as_deref(), Some("main"));
            }
            git(&f.root, &["switch", "-c", "external"]).unwrap();
            assert!(switch_branch(&f.db, &f.selection(None, Some("new")))
                .await
                .unwrap_err()
                .contains("outside"));
            let mut workspace = f.workspace.clone();
            workspace.worktree_path = Some(f.root.clone());
            SqliteWorkspaceRepo::open(&f.db)
                .unwrap()
                .save_workspace(&workspace)
                .await
                .unwrap();
            assert!(switch_branch(&f.db, &f.selection(None, Some("new")))
                .await
                .unwrap_err()
                .contains("local-direct"));
        });
    }

    #[test]
    fn remote_checkout_tracks_branch_and_omits_symbolic_remote_head() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let remote = f._dir.path().join("remote.git");
            let output = std::process::Command::new("git")
                .args(["clone", "--bare", &f.root])
                .arg(&remote)
                .output()
                .unwrap();
            assert!(output.status.success());
            git(
                &f.root,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            )
            .unwrap();
            git(&f.root, &["push", "origin", "main:feature/remote"]).unwrap();
            git(&f.root, &["fetch", "origin"]).unwrap();
            git(
                &f.root,
                &[
                    "symbolic-ref",
                    "refs/remotes/origin/HEAD",
                    "refs/remotes/origin/main",
                ],
            )
            .unwrap();
            let before = status(&f.db, &f.workspace).await.unwrap();
            assert!(!before.branches.iter().any(|b| b.name == "origin/HEAD"));
            let result = switch_branch(
                &f.db,
                &f.selection(Some("refs/remotes/origin/feature/remote"), None),
            )
            .await
            .unwrap();
            assert_eq!(result.current_branch.as_deref(), Some("feature/remote"));
            assert_eq!(
                git(&f.root, &["rev-parse", "--abbrev-ref", "@{upstream}"]).unwrap(),
                "origin/feature/remote"
            );
        });
    }

    #[test]
    fn conflicting_edits_and_branches_used_by_worktrees_are_preserved() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            std::fs::write(Path::new(&f.root).join("file"), "main").unwrap();
            git(&f.root, &["add", "file"]).unwrap();
            git(
                &f.root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "file",
                ],
            )
            .unwrap();
            git(&f.root, &["switch", "-c", "other"]).unwrap();
            std::fs::write(Path::new(&f.root).join("file"), "other").unwrap();
            git(
                &f.root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-am",
                    "other",
                ],
            )
            .unwrap();
            git(&f.root, &["switch", "main"]).unwrap();
            std::fs::write(Path::new(&f.root).join("file"), "unsaved").unwrap();
            assert!(
                switch_branch(&f.db, &f.selection(Some("refs/heads/other"), None))
                    .await
                    .is_err()
            );
            assert_eq!(
                std::fs::read_to_string(Path::new(&f.root).join("file")).unwrap(),
                "unsaved"
            );
            let path = f._dir.path().join("worktree");
            git(
                &f.root,
                &["worktree", "add", path.to_str().unwrap(), "other"],
            )
            .unwrap();
            assert!(
                switch_branch(&f.db, &f.selection(Some("refs/heads/other"), None))
                    .await
                    .is_err()
            );
            assert_eq!(current_branch(&f.root).unwrap().as_deref(), Some("main"));
        });
    }

    #[test]
    fn first_message_locks_task_permanently_and_rejects_external_branch_changes() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let session = f.session("session", &f.workspace).await;
            assert!(!status(&f.db, &f.workspace).await.unwrap().has_conversation);
            f.event(&session, 2, Fixture::started()).await.unwrap();
            f.event(
                &session,
                3,
                SessionEventKind::TurnAborted {
                    turn_id: TurnId("turn".into()),
                    reason: None,
                },
            )
            .await
            .unwrap();
            f.event(
                &session,
                4,
                SessionEventKind::SessionAborted { reason: None },
            )
            .await
            .unwrap();
            let result = status(&f.db, &f.workspace).await.unwrap();
            assert!(result.has_conversation);
            assert!(!result.agent_running);
            assert!(switch_branch(&f.db, &f.selection(None, Some("blocked")))
                .await
                .unwrap_err()
                .contains("conversation"));
            git(&f.root, &["switch", "-c", "external"]).unwrap();
            assert!(f
                .event(&session, 5, Fixture::started())
                .await
                .unwrap_err()
                .contains("belongs to branch `main`"));
            assert_eq!(
                f.sessions
                    .list_events_by_session(&session)
                    .await
                    .unwrap()
                    .len(),
                4
            );
            git(&f.root, &["switch", "main"]).unwrap();
            f.event(&session, 5, Fixture::started()).await.unwrap();
        });
    }

    #[test]
    fn another_running_local_task_blocks_checkout_but_completed_history_does_not() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let mut other = f.workspace.clone();
            other.id = WorkspaceId("other".into());
            SqliteWorkspaceRepo::open(&f.db)
                .unwrap()
                .save_workspace(&other)
                .await
                .unwrap();
            let session = f.session("session", &other).await;
            f.event(&session, 2, Fixture::started()).await.unwrap();
            assert!(switch_branch(&f.db, &f.selection(None, Some("next")))
                .await
                .unwrap_err()
                .contains("agent"));
            f.event(
                &session,
                3,
                SessionEventKind::TurnAborted {
                    turn_id: TurnId("turn".into()),
                    reason: None,
                },
            )
            .await
            .unwrap();
            switch_branch(&f.db, &f.selection(None, Some("next")))
                .await
                .unwrap();
            assert!(f.event(&session, 4, Fixture::started()).await.is_err());
        });
    }

    #[test]
    fn first_message_binds_actual_branch_for_legacy_empty_tasks() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let session = f.session("session", &f.workspace).await;
            git(&f.root, &["switch", "-c", "existing"]).unwrap();
            f.event(&session, 2, Fixture::started()).await.unwrap();
            let stored = local_workspace(&f.db, &f.workspace.id).await.unwrap();
            assert_eq!(stored.base_branch, "existing");
        });
    }

    #[tokio::test(flavor = "current_thread")]
    async fn first_message_waits_for_checkout_on_the_same_physical_root() {
        let f = Fixture::new().await;
        let session = f.session("session", &f.workspace).await;
        let checkout_guard = lock_root(&format!("{}/.", f.root)).await.unwrap();
        let first_message = f.event(&session, 2, Fixture::started());
        tokio::pin!(first_message);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut first_message)
                .await
                .is_err()
        );
        assert_eq!(
            f.sessions
                .list_events_by_session(&session)
                .await
                .unwrap()
                .len(),
            1
        );
        switch_branch(&f.db, &f.selection(None, Some("selected")))
            .await
            .unwrap();
        drop(checkout_guard);
        first_message.await.unwrap();
        let stored = local_workspace(&f.db, &f.workspace.id).await.unwrap();
        assert_eq!(stored.base_branch, "selected");
        assert!(status(&f.db, &stored).await.unwrap().has_conversation);
        let mut selection = f.selection(Some("refs/heads/main"), None);
        selection.expected_branch = Some("selected".into());
        assert!(switch_branch(&f.db, &selection)
            .await
            .unwrap_err()
            .contains("conversation"));
    }

    #[test]
    fn removed_queued_message_still_locks_the_task() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let session = f.session("session", &f.workspace).await;
            f.event(
                &session,
                2,
                SessionEventKind::TurnQueued {
                    queued_turn: dcc_core::domain::session::QueuedTurn {
                        id: "queued".into(),
                        session_id: session.clone(),
                        prompt: "queued work".into(),
                        tool_instructions: None,
                        plan_mode: None,
                        effort: None,
                        fast_mode: None,
                        approval_policy: None,
                        evidence: None,
                        created_at: f.workspace.created_at.clone(),
                    },
                },
            )
            .await
            .unwrap();
            f.event(
                &session,
                3,
                SessionEventKind::QueuedTurnRemoved {
                    queued_turn_id: "queued".into(),
                },
            )
            .await
            .unwrap();
            assert!(switch_branch(&f.db, &f.selection(None, Some("new")))
                .await
                .unwrap_err()
                .contains("conversation"));
        });
    }

    #[test]
    fn detached_head_requires_branch_selection_and_worktree_turns_are_unchanged() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let session = f.session("session", &f.workspace).await;
            git(&f.root, &["switch", "--detach"]).unwrap();
            assert!(f
                .event(&session, 2, Fixture::started())
                .await
                .unwrap_err()
                .contains("detached HEAD"));
            let mut isolated = f.workspace.clone();
            isolated.worktree_path = Some(f.root.clone());
            SqliteWorkspaceRepo::open(&f.db)
                .unwrap()
                .save_workspace(&isolated)
                .await
                .unwrap();
            f.event(&session, 2, Fixture::started()).await.unwrap();
        });
    }

    #[test]
    fn legacy_conversations_adopt_the_current_branch_instead_of_the_old_project_default() {
        futures::executor::block_on(async {
            let f = Fixture::new().await;
            let session = f.session("session", &f.workspace).await;
            git(&f.root, &["switch", "-c", "legacy-work"]).unwrap();
            // Simulate a pre-upgrade event, with no branch binding.
            let sessions = SqliteSessionRepo::open(&f.db).unwrap();
            sessions
                .append_event(&SessionEventRecord {
                    event_id: "legacy-start".into(),
                    session_id: session.clone(),
                    sequence: 2,
                    occurred_at: f.workspace.created_at.clone(),
                    kind: Fixture::started(),
                })
                .await
                .unwrap();
            let result = status(&f.db, &f.workspace).await.unwrap();
            assert!(result.has_conversation);
            assert_eq!(result.task_branch, "legacy-work");
            git(&f.root, &["switch", "main"]).unwrap();
            assert!(guard_conversation_event(&f.db, &session).await.is_err());
            sessions.delete_events_by_session(&session).await.unwrap();
            assert!(status(&f.db, &f.workspace).await.unwrap().has_conversation);
            assert!(switch_branch(&f.db, &f.selection(None, Some("new")))
                .await
                .unwrap_err()
                .contains("conversation"));
        });
    }
}
