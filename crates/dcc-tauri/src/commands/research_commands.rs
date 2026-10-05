//! Ideas researched by the built-in researcher before a project exists.

use std::path::{Path, PathBuf};

use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use dcc_core::domain::idea::{idea_template, Idea, IDEAS_DIR_NAME, IDEA_FOLDER_PREFIX};
use dcc_core::domain::repository::RepositoryId;
use dcc_infra::db::{SqliteSessionRepo, SqliteWorkspaceRepo};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    commands::forge::{github, resolve_cli_binary},
    commands::workspace_commands::delete_repository_with_workspaces,
    commands::workspace_support::run_git_network_output_with_workspace_auth,
    git::{git_output_err, run_git_output},
    state::WorkspaceCommandState,
};

const IDEA_FILE: &str = "IDEIA.md";
const PUBLISHED_IDEA_FILE: &str = "docs/IDEIA.md";
const MAX_FOLDER_ATTEMPTS: usize = 1_000;
const INITIAL_COMMIT_MESSAGE: &str = "docs: start idea";
const SAVE_COMMIT_MESSAGE: &str = "docs: update idea";
const PUBLISH_COMMIT_MESSAGE: &str = "chore: start project from idea";
const BACKUP_BRANCH: &str = "dcc-idea-backup";
const GITHUB_HOST: &str = "github.com";
const GH_TIMEOUT: Duration = Duration::from_secs(60);

/// What every task of a published project reads first. Fixed text, not
/// written by a model: it only points to the research.
fn agents_instructions(name: &str) -> String {
    format!(
        "# {name}\n\n\
Este projeto nasceu de uma pesquisa registrada em `{PUBLISHED_IDEA_FILE}`: problema, público,\n\
hipóteses, arquitetura, decisões e roadmap. Leia antes de começar uma tarefa e siga o roadmap.\n\
Quando uma decisão do projeto mudar algo de lá, atualize o `{PUBLISHED_IDEA_FILE}`.\n"
    )
}

/// Claude Code reads `CLAUDE.md`; it loads the same instructions.
const CLAUDE_INSTRUCTIONS: &str = "@AGENTS.md\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdeaVisibility {
    Private,
    Public,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeaRootInput {
    pub root_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishIdeaInput {
    pub root_path: String,
    /// Repository name, also the project's folder name.
    pub name: String,
    /// Folder that receives the project folder.
    pub destination: String,
    pub visibility: IdeaVisibility,
    /// Public repository only: keep `docs/IDEIA.md` local, out of every commit.
    #[serde(default)]
    pub keep_idea_local: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedIdea {
    pub root_path: String,
    pub repository_url: Option<String>,
}

fn ideas_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(IDEAS_DIR_NAME))
        .ok_or_else(|| "could not find the home folder for the ideas".to_string())
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let root = root.to_string_lossy();
    let output = run_git_output(&root, args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_output_err(
            &format!("git {}", args.first().copied().unwrap_or_default()),
            &output.stderr,
        ))
    }
}

/// Claims a new folder for the idea: `ideia`, or `ideia-<n>` when older ideas
/// took the earlier names. `create_dir` fails on an existing folder, so two
/// ideas never share one.
fn claim_idea_folder(base: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(base)
        .map_err(|error| format!("could not create {}: {error}", base.display()))?;
    for attempt in 1..=MAX_FOLDER_ATTEMPTS {
        let name = if attempt == 1 {
            IDEA_FOLDER_PREFIX.to_string()
        } else {
            format!("{IDEA_FOLDER_PREFIX}-{attempt}")
        };
        let path = base.join(name);
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("could not create {}: {error}", path.display()));
            }
        }
    }
    Err(format!("too many ideas in {}", base.display()))
}

/// The idea's repository: `IDEIA.md` from the template in one first commit,
/// so the task starts on a normal branch. The person's git identity is used;
/// without one, the commit is signed as DCC.
fn init_idea_repository(root: &Path) -> Result<(), String> {
    std::fs::write(root.join(IDEA_FILE), idea_template())
        .map_err(|error| format!("could not write {IDEA_FILE}: {error}"))?;
    git(root, &["init", "-b", "main"])?;
    git(root, &["add", IDEA_FILE])?;
    commit(root, INITIAL_COMMIT_MESSAGE)
}

/// Commits what is staged with the person's git identity, or as DCC when
/// git has none.
fn commit(root: &Path, message: &str) -> Result<(), String> {
    if git(root, &["config", "user.email"]).is_ok() {
        git(root, &["commit", "-q", "-m", message])
    } else {
        git(
            root,
            &[
                "-c",
                "user.name=DCC",
                "-c",
                "user.email=dcc@localhost",
                "commit",
                "-q",
                "-m",
                message,
            ],
        )
    }
}

/// Creates the folder and repository of a new idea under `base`. A folder
/// this call created is removed again when a later step fails.
pub fn create_idea_folder(base: &Path) -> Result<PathBuf, String> {
    let root = claim_idea_folder(base)?;
    let created = init_idea_repository(&root).and_then(|()| {
        root.canonicalize()
            .map_err(|error| format!("could not resolve {}: {error}", root.display()))
    });
    if created.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    created
}

/// Starts an idea: its folder in `~/dcc-ideias`, its empty `IDEIA.md` and the
/// mark that keeps it out of the project list. The app then opens its task,
/// where the person describes the idea to the researcher.
#[tauri::command]
pub async fn research_create_idea(state: State<'_, WorkspaceCommandState>) -> Result<Idea, String> {
    let base = ideas_dir()?;
    let root = tokio::task::spawn_blocking(move || create_idea_folder(&base))
        .await
        .map_err(|error| error.to_string())??;
    let idea = Idea {
        root_path: root.to_string_lossy().into_owned(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let saved = SqliteWorkspaceRepo::open(&state.db_path)
        .and_then(|repo| repo.save_idea(&idea))
        .map_err(|error| error.to_string());
    if let Err(error) = saved {
        let _ = std::fs::remove_dir_all(&root);
        return Err(error);
    }
    Ok(idea)
}

fn is_valid_repository_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The idea's folder, only when it is a registered idea inside `~/dcc-ideias`:
/// publishing moves it and discarding deletes it, so nothing else qualifies.
fn registered_idea_root(repo: &SqliteWorkspaceRepo, root_path: &str) -> Result<PathBuf, String> {
    let registered = repo
        .list_ideas()
        .map_err(|error| error.to_string())?
        .into_iter()
        .any(|idea| idea.root_path == root_path);
    if !registered {
        return Err("this folder is not an idea in progress".to_string());
    }
    let base = ideas_dir()?;
    let root = Path::new(root_path)
        .canonicalize()
        .map_err(|error| format!("could not find the idea folder: {error}"))?;
    let base = base.canonicalize().unwrap_or(base);
    if root.parent() != Some(base.as_path()) {
        return Err(format!("the idea is not inside {}", base.display()));
    }
    Ok(root)
}

/// Rebuilds the idea as the project's first commit on `main`: `IDEIA.md` moves
/// to `docs/`, and `AGENTS.md` and `CLAUDE.md` point every future task to it.
/// The research history stays on a backup branch until the publish finishes,
/// so `rollback_publish_branch` can restore the idea exactly.
fn prepare_publish_branch(root: &Path, name: &str, keep_idea_local: bool) -> Result<(), String> {
    git(root, &["add", "-A"])?;
    if !git_succeeds(root, &["diff", "--cached", "--quiet"]) {
        commit(root, SAVE_COMMIT_MESSAGE)?;
    }
    git(root, &["branch", "-M", BACKUP_BRANCH])?;
    git(root, &["checkout", "-q", "--orphan", "main"])?;
    git(root, &["rm", "-r", "-q", "--cached", "."])?;
    std::fs::create_dir_all(root.join("docs"))
        .map_err(|error| format!("could not create docs: {error}"))?;
    std::fs::rename(root.join(IDEA_FILE), root.join(PUBLISHED_IDEA_FILE))
        .map_err(|error| format!("could not move {IDEA_FILE}: {error}"))?;
    if !root.join("AGENTS.md").exists() {
        std::fs::write(root.join("AGENTS.md"), agents_instructions(name))
            .map_err(|error| format!("could not write AGENTS.md: {error}"))?;
    }
    if !root.join("CLAUDE.md").exists() {
        std::fs::write(root.join("CLAUDE.md"), CLAUDE_INSTRUCTIONS)
            .map_err(|error| format!("could not write CLAUDE.md: {error}"))?;
    }
    if keep_idea_local {
        let exclude = root.join(".git/info/exclude");
        let mut text = std::fs::read_to_string(&exclude).unwrap_or_default();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(PUBLISHED_IDEA_FILE);
        text.push('\n');
        std::fs::create_dir_all(root.join(".git/info"))
            .and_then(|()| std::fs::write(&exclude, text))
            .map_err(|error| format!("could not keep {PUBLISHED_IDEA_FILE} local: {error}"))?;
    }
    git(root, &["add", "-A"])?;
    commit(root, PUBLISH_COMMIT_MESSAGE)
}

fn git_succeeds(root: &Path, args: &[&str]) -> bool {
    run_git_output(&root.to_string_lossy(), args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Puts the idea back as it was before `prepare_publish_branch`.
fn rollback_publish_branch(root: &Path) {
    let _ = git(root, &["checkout", "-q", "-f", BACKUP_BRANCH]);
    let _ = git(root, &["branch", "-D", "main"]);
    let _ = git(root, &["branch", "-M", "main"]);
    let _ = git(root, &["remote", "remove", "origin"]);
    let exclude = root.join(".git/info/exclude");
    if let Ok(text) = std::fs::read_to_string(&exclude) {
        let kept: String = text
            .lines()
            .filter(|line| *line != PUBLISHED_IDEA_FILE)
            .map(|line| format!("{line}\n"))
            .collect();
        let _ = std::fs::write(&exclude, kept);
    }
    // A local-only docs/IDEIA.md was never tracked, so checkout left it.
    let _ = std::fs::remove_file(root.join(PUBLISHED_IDEA_FILE));
    let _ = std::fs::remove_dir(root.join("docs"));
}

/// Publishes the idea folder `root` as `target`: rebuilds `main`, runs
/// `remote` (create the repository and push) from the idea folder, and only
/// then moves the folder. Any failure before the move restores the idea.
fn publish_idea_folder(
    root: &Path,
    target: &Path,
    name: &str,
    keep_idea_local: bool,
    remote: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if let Err(error) =
        prepare_publish_branch(root, name, keep_idea_local).and_then(|()| remote(root))
    {
        rollback_publish_branch(root);
        return Err(error);
    }
    let _ = git(root, &["branch", "-D", BACKUP_BRANCH]);
    std::fs::rename(root, target).map_err(|error| {
        format!(
            "the repository was published, but the folder could not be moved to {}: {error}",
            target.display()
        )
    })
}

fn run_gh(gh: &Path, cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    dcc_infra::process::run_command_with_timeout(
        gh,
        |command| {
            command.current_dir(cwd).args(args);
        },
        GH_TIMEOUT,
    )
}

fn output_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}

/// Same disk: the folder is moved, never copied, so nothing is left behind.
#[cfg(unix)]
fn same_disk(left: &Path, right: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(left), std::fs::metadata(right)) {
        (Ok(left), Ok(right)) => left.dev() == right.dev(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_disk(_left: &Path, _right: &Path) -> bool {
    true
}

/// Publishes an idea: checks everything that can fail first, rebuilds its
/// first commit with `docs/IDEIA.md`, `AGENTS.md` and `CLAUDE.md`, creates the
/// GitHub repository and pushes, moves the folder to `destination/name`, and
/// turns the idea into a normal project. The app then opens a task in it.
#[tauri::command]
pub async fn research_publish_idea(
    state: State<'_, WorkspaceCommandState>,
    input: PublishIdeaInput,
) -> Result<PublishedIdea, String> {
    let name = input.name.trim().to_string();
    if !is_valid_repository_name(&name) {
        return Err("use letters, numbers, '.', '_' or '-' for the repository name".to_string());
    }
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let root = registered_idea_root(&repo, &input.root_path)?;
    if !root.join(IDEA_FILE).is_file() {
        return Err(format!("{IDEA_FILE} is missing from the idea folder"));
    }
    let destination = PathBuf::from(input.destination.trim());
    if !destination.is_dir() {
        return Err(format!(
            "the folder {} does not exist",
            destination.display()
        ));
    }
    let target = destination.join(&name);
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    if !same_disk(&root, &destination) {
        return Err("choose a folder on the same disk as ~/dcc-ideias".to_string());
    }
    let keep_idea_local = input.keep_idea_local && input.visibility == IdeaVisibility::Public;
    let visibility_flag = match input.visibility {
        IdeaVisibility::Private => "--private",
        IdeaVisibility::Public => "--public",
    };
    let db_path = state.db_path.clone();
    let publish_root = root.clone();
    let publish_target = target.clone();
    let publish_name = name.clone();
    let repository_url = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let gh = resolve_cli_binary("gh")?;
        let auth = github::auth_status(GITHUB_HOST)?;
        let login = auth
            .active_login
            .clone()
            .or_else(|| auth.logins.first().cloned())
            .ok_or_else(|| "connect the GitHub CLI first: gh auth login".to_string())?;
        let exists = run_gh(&gh, &publish_root, &["repo", "view", &publish_name, "--json", "name"])?;
        if exists.status.success() {
            return Err(format!("the repository {login}/{publish_name} already exists on GitHub"));
        }
        let mut url = None;
        publish_idea_folder(
            &publish_root,
            &publish_target,
            &publish_name,
            keep_idea_local,
            |root| {
                let created = run_gh(
                    &gh,
                    root,
                    &[
                        "repo",
                        "create",
                        &publish_name,
                        visibility_flag,
                        "--source",
                        ".",
                        "--remote",
                        "origin",
                    ],
                )?;
                if !created.status.success() {
                    return Err(format!(
                        "could not create the repository: {}",
                        output_detail(&created)
                    ));
                }
                url = String::from_utf8_lossy(&created.stdout)
                    .lines()
                    .map(str::trim)
                    .find(|line| line.starts_with("https://"))
                    .map(ToString::to_string);
                let pushed = run_git_network_output_with_workspace_auth(
                    &db_path,
                    &root.to_string_lossy(),
                    &["push", "-u", "origin", "main"],
                    Some(&login),
                )?;
                if !pushed.status.success() {
                    return Err(format!(
                        "the repository {login}/{publish_name} was created on GitHub, but the push failed: {}. Delete it on GitHub or choose another name.",
                        output_detail(&pushed)
                    ));
                }
                Ok(())
            },
        )?;
        Ok(url)
    })
    .await
    .map_err(|error| error.to_string())??;

    let old_root = input.root_path.clone();
    let new_root = target.to_string_lossy().into_owned();
    if let Err(error) = repo.publish_idea(&old_root, &new_root, &name) {
        // Keep the folder where the records still point.
        let _ = std::fs::rename(&target, &root);
        return Err(error.to_string());
    }
    state.clear_delivery_failures(&old_root);
    Ok(PublishedIdea {
        root_path: new_root,
        repository_url,
    })
}

/// Discards an idea: its task, the researcher's conversation, the records
/// and the folder in `~/dcc-ideias`. Nothing is kept.
#[tauri::command]
pub async fn research_discard_idea(
    state: State<'_, WorkspaceCommandState>,
    input: IdeaRootInput,
) -> Result<(), String> {
    let repo = SqliteWorkspaceRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let session_repo =
        SqliteSessionRepo::open(&state.db_path).map_err(|error| error.to_string())?;
    let root = registered_idea_root(&repo, &input.root_path)?;
    let id = RepositoryId(input.root_path.clone());
    let removed =
        delete_repository_with_workspaces(&state, &repo, &session_repo, &id, &state.db_path)
            .await?;
    for workspace in removed {
        state.clear_delivery_failures(&workspace.root_path);
    }
    std::fs::remove_dir_all(&root)
        .map_err(|error| format!("could not delete {}: {error}", root.display()))
}

/// Ideas in progress, newest first.
#[tauri::command]
pub async fn research_list_ideas(
    state: State<'_, WorkspaceCommandState>,
) -> Result<Vec<Idea>, String> {
    SqliteWorkspaceRepo::open(&state.db_path)
        .and_then(|repo| repo.list_ideas())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idea_folder_is_a_repository_with_the_template_committed() {
        let base = tempfile::tempdir().expect("ideas dir");
        let root = create_idea_folder(base.path()).expect("create idea");
        assert_eq!(root.file_name().unwrap(), "ideia");
        let text = std::fs::read_to_string(root.join(IDEA_FILE)).expect("read idea");
        assert_eq!(text, idea_template());
        let root_text = root.to_string_lossy();
        let status = run_git_output(&root_text, &["status", "--porcelain"]).expect("status");
        assert!(status.status.success());
        assert!(status.stdout.is_empty(), "the template is committed");
        let branch =
            run_git_output(&root_text, &["rev-parse", "--abbrev-ref", "HEAD"]).expect("branch");
        assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "main");

        // The next idea gets its own folder.
        let again = create_idea_folder(base.path()).expect("create second idea");
        assert_eq!(again.file_name().unwrap(), "ideia-2");
    }

    fn tracked_files(root: &Path) -> Vec<String> {
        let output = run_git_output(&root.to_string_lossy(), &["ls-files"]).expect("ls-files");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(ToString::to_string)
            .collect()
    }

    fn commit_count(root: &Path) -> usize {
        let output = run_git_output(&root.to_string_lossy(), &["rev-list", "--count", "HEAD"])
            .expect("count");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap()
    }

    #[test]
    fn publish_moves_the_folder_with_one_commit_pointing_to_the_research() {
        let base = tempfile::tempdir().expect("ideas dir");
        let projects = tempfile::tempdir().expect("projects dir");
        let root = create_idea_folder(base.path()).expect("create idea");
        // The researcher edits IDEIA.md without committing.
        std::fs::write(root.join(IDEA_FILE), "# Ideia: clínica via chat\n").unwrap();
        let target = projects.path().join("salte");

        publish_idea_folder(&root, &target, "salte", false, |_| Ok(())).expect("publish");

        assert!(!root.exists());
        assert_eq!(
            std::fs::read_to_string(target.join(PUBLISHED_IDEA_FILE)).unwrap(),
            "# Ideia: clínica via chat\n"
        );
        assert!(std::fs::read_to_string(target.join("AGENTS.md"))
            .unwrap()
            .contains("`docs/IDEIA.md`"));
        assert_eq!(
            std::fs::read_to_string(target.join("CLAUDE.md")).unwrap(),
            "@AGENTS.md\n"
        );
        assert_eq!(
            tracked_files(&target),
            vec!["AGENTS.md", "CLAUDE.md", "docs/IDEIA.md"]
        );
        assert_eq!(commit_count(&target), 1);
        assert!(!git_succeeds(
            &target,
            &["rev-parse", "--verify", BACKUP_BRANCH]
        ));
    }

    #[test]
    fn public_idea_kept_local_stays_out_of_the_commit() {
        let base = tempfile::tempdir().expect("ideas dir");
        let projects = tempfile::tempdir().expect("projects dir");
        let root = create_idea_folder(base.path()).expect("create idea");
        let target = projects.path().join("salte");

        publish_idea_folder(&root, &target, "salte", true, |_| Ok(())).expect("publish");

        assert!(target.join(PUBLISHED_IDEA_FILE).is_file());
        assert_eq!(tracked_files(&target), vec!["AGENTS.md", "CLAUDE.md"]);
    }

    #[test]
    fn failed_remote_restores_the_idea_with_its_edits() {
        let base = tempfile::tempdir().expect("ideas dir");
        let projects = tempfile::tempdir().expect("projects dir");
        let root = create_idea_folder(base.path()).expect("create idea");
        std::fs::write(root.join(IDEA_FILE), "# Ideia: clínica via chat\n").unwrap();
        let target = projects.path().join("salte");

        let error = publish_idea_folder(&root, &target, "salte", true, |_| {
            Err("push failed".to_string())
        })
        .unwrap_err();

        assert_eq!(error, "push failed");
        assert!(!target.exists());
        assert_eq!(
            std::fs::read_to_string(root.join(IDEA_FILE)).unwrap(),
            "# Ideia: clínica via chat\n"
        );
        assert!(!root.join("docs").exists());
        assert!(!root.join("AGENTS.md").exists());
        assert_eq!(tracked_files(&root), vec![IDEA_FILE]);
        let branch = run_git_output(
            &root.to_string_lossy(),
            &["rev-parse", "--abbrev-ref", "HEAD"],
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "main");
        assert!(!std::fs::read_to_string(root.join(".git/info/exclude"))
            .unwrap_or_default()
            .contains(PUBLISHED_IDEA_FILE));
    }

    #[test]
    fn repository_names_follow_github() {
        assert!(is_valid_repository_name("salte-saude.app_2"));
        for name in ["", ".", "..", "salte saude", "sálte", "a/b"] {
            assert!(!is_valid_repository_name(name), "{name}");
        }
    }
}
