//! The open pull requests the viewer reviewed and what came on each after
//! the review (GitHub): the commits pushed and the author's comments, and
//! when the latest came. One search for every repository
//! (`reviewed-by:@me`), so the app's watcher costs one `gh` however many
//! projects there are; the hub's list is not touched. The search's reviews
//! carry no commit, so the times decide, as the code tab's "Desde sua
//! revisão" does.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::state::WorkspaceCommandState;

const HOST: &str = "github.com";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubSinceReview {
    /// When the viewer last submitted a review.
    pub reviewed_at: String,
    pub commits: u32,
    /// The pull request author's comments on it.
    pub replies: u32,
    /// The latest of those commits and comments.
    pub last_activity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubReviewed {
    pub url: String,
    pub number: u32,
    /// `owner/name`
    pub repository: String,
    pub title: String,
    pub author: Option<String>,
    /// The viewer's latest review: `APPROVED`, `CHANGES_REQUESTED`,
    /// `COMMENTED` or `DISMISSED`.
    pub state: String,
    pub since: PullRequestHubSinceReview,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubReviewedOutput {
    pub items: Vec<PullRequestHubReviewed>,
    pub warnings: Vec<String>,
    /// The person's own open pull requests whose checks fail, each with its
    /// task when DCC has one; read in the same call (`pr_hub_failing`).
    #[serde(default)]
    pub failing: Vec<super::pr_hub_failing::PullRequestHubFailing>,
    /// The person's own open pull requests someone else reviewed or
    /// commented on, each with its task (`pr_hub_received`).
    #[serde(default)]
    pub received: Vec<super::pr_hub_received::PullRequestHubReceived>,
}

fn at(value: Option<&Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn stamp(time: &DateTime<Utc>) -> String {
    time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn login_of(value: &Value) -> Option<&str> {
    value.get("author")?.get("login")?.as_str()
}

fn nodes<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// One pull request of the search, with what came after `login`'s latest
/// review; `None` without one (a pending draft is not a review).
fn reviewed(node: &Value, login: &str) -> Option<PullRequestHubReviewed> {
    let mine = |value: &&Value| login_of(value).is_some_and(|by| by.eq_ignore_ascii_case(login));
    let (reviewed, state) = nodes(node, "latestReviews")
        .iter()
        .filter(mine)
        .filter_map(|review| {
            let state = review.get("state").and_then(Value::as_str)?;
            (state != "PENDING").then_some((at(review.get("submittedAt"))?, state))
        })
        .max_by_key(|(time, _)| *time)?;
    let commits: Vec<DateTime<Utc>> = nodes(node, "commits")
        .iter()
        .filter_map(|entry| at(entry.get("commit")?.get("committedDate")))
        .filter(|time| *time > reviewed)
        .collect();
    let author = login_of(node);
    let replies: Vec<DateTime<Utc>> = nodes(node, "comments")
        .iter()
        .filter(|comment| author.is_some() && login_of(comment) == author && !mine(comment))
        .filter_map(|comment| at(comment.get("createdAt")))
        .filter(|time| *time > reviewed)
        .collect();
    let last = commits.iter().chain(&replies).max();
    Some(PullRequestHubReviewed {
        url: node.get("url")?.as_str()?.to_string(),
        number: node.get("number")?.as_u64()? as u32,
        repository: node
            .get("repository")
            .and_then(|repository| repository.get("nameWithOwner"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        title: node
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        author: author.map(str::to_string),
        state: state.to_string(),
        since: PullRequestHubSinceReview {
            reviewed_at: stamp(&reviewed),
            commits: commits.len() as u32,
            replies: replies.len() as u32,
            last_activity: last.map(stamp),
        },
    })
}

/// The search's answer: whose it is, and its pull requests.
pub(crate) fn from_search(answer: &Value) -> (Option<String>, Vec<PullRequestHubReviewed>) {
    let data = answer.get("data").unwrap_or(answer);
    let viewer = data
        .get("viewer")
        .and_then(|viewer| viewer.get("login"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let items = match &viewer {
        Some(login) => data
            .get("search")
            .and_then(|search| search.get("nodes"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|node| reviewed(node, login))
            .collect(),
        None => Vec::new(),
    };
    (viewer, items)
}

/// The CLI's active account, and the account DCC chose for GitHub when it is
/// another one (each search is one `gh`; another account costs its token).
pub async fn pull_request_hub_reviewed_with_state(
    state: &WorkspaceCommandState,
) -> Result<PullRequestHubReviewedOutput, String> {
    let db_path = state.db_path.clone();
    let chosen = dcc_infra::db::SqliteWorkspaceRepo::open(&state.db_path)
        .ok()
        .and_then(|repo| repo.get_forge_login_preference("github", HOST).ok())
        .flatten();
    tokio::task::spawn_blocking(move || {
        let mut output = PullRequestHubReviewedOutput::default();
        let viewer = match dcc_infra::forge::github::reviewed_pull_requests_json(HOST, None) {
            Ok(answer) => {
                let (viewer, items) = from_search(&answer);
                output.items.extend(items);
                output
                    .failing
                    .extend(super::pr_hub_failing::from_search(&answer));
                output
                    .received
                    .extend(super::pr_hub_received::from_search(&answer));
                viewer
            }
            Err(error) => {
                output.warnings.push(error);
                None
            }
        };
        let other = chosen.filter(|chosen| {
            viewer
                .as_deref()
                .is_none_or(|viewer| !viewer.eq_ignore_ascii_case(chosen))
        });
        if let Some(login) = other {
            match dcc_infra::forge::github::reviewed_pull_requests_json(HOST, Some(&login)) {
                Ok(answer) => {
                    let (_, items) = from_search(&answer);
                    for item in items {
                        if !output.items.iter().any(|known| known.url == item.url) {
                            output.items.push(item);
                        }
                    }
                    for pr in super::pr_hub_failing::from_search(&answer) {
                        if !output.failing.iter().any(|known| known.url == pr.url) {
                            output.failing.push(pr);
                        }
                    }
                    for pr in super::pr_hub_received::from_search(&answer) {
                        if !output.received.iter().any(|known| known.url == pr.url) {
                            output.received.push(pr);
                        }
                    }
                }
                Err(error) => output.warnings.push(error),
            }
        }
        link_tasks(&db_path, &mut output);
        output
    })
    .await
    .map_err(|error| error.to_string())
}

/// Each failing or reviewed pull request's task, from DCC's database and,
/// only for what the saved branches do not settle, the checkouts' local git.
fn link_tasks(db_path: &std::path::Path, output: &mut PullRequestHubReviewedOutput) {
    use dcc_core::ports::{RepositoryRepo, WorkspaceRepo};
    if output.failing.is_empty() && output.received.is_empty() {
        return;
    }
    let Ok(repo) = dcc_infra::db::SqliteWorkspaceRepo::open(db_path) else {
        return;
    };
    let repositories = futures::executor::block_on(repo.list_repositories()).unwrap_or_default();
    let workspaces = futures::executor::block_on(repo.list_workspaces()).unwrap_or_default();
    let branch_of = |path: &str| {
        dcc_infra::workspace_git::read_branch_and_conflicts(path)
            .ok()
            .and_then(|(branch, _)| branch)
    };
    super::pr_hub_failing::link(&mut output.failing, &repositories, &workspaces, branch_of);
    super::pr_hub_received::link(&mut output.received, &repositories, &workspaces, branch_of);
}

#[cfg(test)]
#[path = "pr_hub_since_tests.rs"]
mod tests;
