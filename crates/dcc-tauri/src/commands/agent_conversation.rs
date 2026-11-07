//! The home of a resident agent's own conversation
//! (`dcc_core::domain::agent_conversation`): `<app data>/agent-homes/<id>`,
//! an empty repository with one empty commit so the task starts on `main`
//! like any other. Made on the first "Conversar" and reused after that.

use std::path::{Path, PathBuf};

use dcc_core::domain::agent_conversation::{home_project_id, is_valid_agent_id, AGENT_HOMES_DIR};
use serde::Serialize;

use crate::git::{git_output_err, run_git_output};
use crate::state::SessionCommandState;

const INITIAL_COMMIT_MESSAGE: &str = "chore: agent home";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHome {
    pub root_path: String,
    pub project_id: String,
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = run_git_output(&root.to_string_lossy(), args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_output_err(
            &format!("git {}", args.first().copied().unwrap_or_default()),
            &output.stderr,
        ))
    }
}

/// The agent's home under `app_data`, made when missing. A folder whose
/// repository failed to start is left for the next call to finish.
pub fn ensure_agent_home(app_data: &Path, agent_id: &str) -> Result<PathBuf, String> {
    if !is_valid_agent_id(agent_id) {
        return Err(format!("invalid agent id: {agent_id}"));
    }
    let root = app_data.join(AGENT_HOMES_DIR).join(agent_id);
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("could not create {}: {error}", root.display()))?;
    if !root.join(".git").exists() {
        git(&root, &["init", "-q", "-b", "main"])?;
    }
    let has_commit = run_git_output(
        &root.to_string_lossy(),
        &["rev-parse", "-q", "--verify", "HEAD"],
    )
    .is_ok_and(|output| output.status.success());
    if !has_commit {
        git(
            &root,
            &[
                "-c",
                "user.name=DCC",
                "-c",
                "user.email=dcc@localhost",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                INITIAL_COMMIT_MESSAGE,
            ],
        )?;
    }
    root.canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", root.display()))
}

/// The home of a live agent, beside DCC's database.
pub async fn agent_home_with_state(
    state: &SessionCommandState,
    agent_id: &str,
) -> Result<AgentHome, String> {
    let agent_id = agent_id.trim().to_string();
    if state
        .resident_agent(&agent_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!("agent {agent_id} does not exist"));
    }
    let app_data = state
        .db_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "could not find DCC's data folder".to_string())?;
    let id = agent_id.clone();
    let root = tokio::task::spawn_blocking(move || ensure_agent_home(&app_data, &id))
        .await
        .map_err(|error| error.to_string())??;
    Ok(AgentHome {
        root_path: root.to_string_lossy().into_owned(),
        project_id: home_project_id(&agent_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_is_made_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let first = ensure_agent_home(dir.path(), "a-1").unwrap();
        assert!(first.ends_with("agent-homes/a-1"));
        assert!(first.join(".git").exists());
        let again = ensure_agent_home(dir.path(), "a-1").unwrap();
        assert_eq!(first, again);
        let log =
            run_git_output(&first.to_string_lossy(), &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "1");
        assert!(ensure_agent_home(dir.path(), "../escape").is_err());
    }
}
