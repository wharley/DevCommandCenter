//! The branch a pull request comes from, read from GitHub: one `gh pr
//! view`, to tell whether work asked about a pull request already has a
//! task (`dcc_core::domain::task_same_work`).

use super::github_read::view_argv;
use super::github_read_run::{pull_request_view, str_at, Gh, GithubTarget};

/// The pull request's head branch; none when GitHub does not say.
pub fn pull_request_head(target: &GithubTarget, number: u64) -> Result<Option<String>, String> {
    let gh = Gh::new(target)?;
    let pr = pull_request_view(&gh.json(&view_argv(&target.repo, number))?);
    Ok(str_at(&pr, "/head"))
}
