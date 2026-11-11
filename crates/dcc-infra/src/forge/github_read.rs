//! What a resident agent may read from GitHub through DCC (`dcc_github`):
//! pull requests, one pull request's detail and a commit's deployments. Every
//! input is checked here and every `gh` argument list is built here from
//! fixed parts, so the agent's text only ever fills a checked slot: no shell,
//! no write subcommand, no `gh api` path or method of its choosing.

use chrono::{DateTime, NaiveDate, SecondsFormat};
use serde::Deserialize;

/// The tool's name; `agent_access` puts every `dcc_github*` in "github".
pub const GITHUB_TOOL: &str = "dcc_github";
pub const MAX_LIMIT: u32 = 100;
const DEFAULT_LIST_LIMIT: u32 = 30;
const DEFAULT_DEPLOY_LIMIT: u32 = 10;
const MAX_TEXT: usize = 256;

const LIST_FIELDS: &str = "number,title,url,state,isDraft,author,baseRefName,headRefName,createdAt,updatedAt,mergedAt,closedAt,mergedBy,mergeCommit,reviewDecision";
const VIEW_FIELDS: &str = "number,title,url,state,isDraft,author,baseRefName,headRefName,headRefOid,mergeable,mergeStateStatus,reviewDecision,latestReviews,reviewRequests,statusCheckRollup,changedFiles,additions,deletions,createdAt,updatedAt,mergedAt,mergedBy,mergeCommit,closedAt";
const ACCEPT: &str = "Accept: application/vnd.github+json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    PullRequests,
    PullRequest,
    Deployments,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    #[default]
    Open,
    Merged,
    Closed,
    All,
}

impl PrState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }

    /// The search qualifier a time window filters on.
    fn time_qualifier(self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Open | Self::All => "updated",
        }
    }
}

/// The tool's input as the agent sends it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GithubReadArgs {
    pub operation: Operation,
    pub repo: Option<String>,
    pub state: Option<PrState>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub author: Option<String>,
    pub base: Option<String>,
    pub search: Option<String>,
    pub approved_by_me: Option<bool>,
    pub number: Option<u64>,
    pub sha: Option<String>,
    pub environment: Option<String>,
    pub limit: Option<u32>,
}

/// A repository as `owner/name`, both checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoSlug {
    pub owner: String,
    pub name: String,
}

impl RepoSlug {
    pub fn parse(value: &str) -> Option<Self> {
        let (owner, name) = value.trim().split_once('/')?;
        (is_slug_part(owner) && is_slug_part(name)).then(|| Self {
            owner: owner.to_string(),
            name: name.trim_end_matches(".git").to_string(),
        })
    }

    pub fn full(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

fn is_slug_part(part: &str) -> bool {
    (1..=100).contains(&part.len())
        && part != "."
        && part != ".."
        && !part.starts_with('-')
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListQuery {
    pub state: PrState,
    /// `merged:2026-11-09T11:00:00-03:00..`-style qualifier, already built.
    pub window: Option<String>,
    pub author: Option<String>,
    pub base: Option<String>,
    pub search: Option<String>,
    pub approved_by_me: bool,
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeployTarget {
    Sha(String),
    PullRequest(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployQuery {
    pub target: Option<DeployTarget>,
    pub environment: Option<String>,
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GithubRead {
    PullRequests(ListQuery),
    PullRequest(u64),
    Deployments(DeployQuery),
}

/// A date (`2026-11-09`) or a moment with its offset, as GitHub search reads it.
pub fn check_time(value: &str) -> Option<String> {
    let value = value.trim();
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(date.format("%Y-%m-%d").to_string());
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|moment| moment.to_rfc3339_opts(SecondsFormat::Secs, false))
}

/// A GitHub login, an app (`app/name`) or `@me`.
pub fn check_login(value: &str) -> Option<String> {
    let value = value.trim();
    let login = value.strip_prefix("app/").unwrap_or(value);
    let ok = value == "@me"
        || ((1..=39).contains(&login.len())
            && !login.starts_with('-')
            && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    ok.then(|| value.to_string())
}

pub fn check_branch(value: &str) -> Option<String> {
    let value = value.trim();
    let ok = (1..=200).contains(&value.len())
        && !value.starts_with(['-', '/'])
        && !value.contains("..")
        && !value.contains("//")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'));
    ok.then(|| value.to_string())
}

/// Free search words; qualifiers that would widen the search past the
/// resolved repository are refused.
pub fn check_search(value: &str) -> Option<String> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let widens = lower.split_whitespace().any(|word| {
        ["repo:", "org:", "user:"]
            .iter()
            .any(|q| word.starts_with(q))
    });
    let ok = !value.is_empty()
        && value.len() <= MAX_TEXT
        && !value.chars().any(char::is_control)
        && !widens;
    ok.then(|| value.to_string())
}

pub fn check_sha(value: &str) -> Option<String> {
    let value = value.trim();
    ((7..=40).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

pub fn check_environment(value: &str) -> Option<String> {
    let value = value.trim();
    ((1..=64).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '.' | '-')))
    .then(|| value.to_string())
}

fn check_number(number: u64) -> Result<u64, String> {
    (1..=99_999_999)
        .contains(&number)
        .then_some(number)
        .ok_or_else(|| "`number` must be a pull request number.".to_string())
}

fn check_limit(limit: Option<u32>, default: u32) -> Result<u32, String> {
    let limit = limit.unwrap_or(default);
    (1..=MAX_LIMIT)
        .contains(&limit)
        .then_some(limit)
        .ok_or_else(|| format!("`limit` must be 1 to {MAX_LIMIT}."))
}

fn checked<T>(
    value: Option<&String>,
    check: fn(&str) -> Option<T>,
    what: &str,
) -> Result<Option<T>, String> {
    value
        .map(|value| check(value).ok_or_else(|| format!("`{what}` is not valid: {value:?}.")))
        .transpose()
}

fn window(state: PrState, since: Option<String>, until: Option<String>) -> Option<String> {
    let qualifier = state.time_qualifier();
    match (since, until) {
        (Some(since), Some(until)) => Some(format!("{qualifier}:{since}..{until}")),
        (Some(since), None) => Some(format!("{qualifier}:>={since}")),
        (None, Some(until)) => Some(format!("{qualifier}:<={until}")),
        (None, None) => None,
    }
}

/// The fields the operation does not take, so a misplaced one is refused.
fn unused(args: &GithubReadArgs, operation: Operation) -> Vec<&'static str> {
    let list = operation == Operation::PullRequests;
    let deploy = operation == Operation::Deployments;
    [
        ("state", args.state.is_some() && !list),
        ("since", args.since.is_some() && !list),
        ("until", args.until.is_some() && !list),
        ("author", args.author.is_some() && !list),
        ("base", args.base.is_some() && !list),
        ("search", args.search.is_some() && !list),
        ("approvedByMe", args.approved_by_me.is_some() && !list),
        ("number", args.number.is_some() && list),
        ("sha", args.sha.is_some() && !deploy),
        ("environment", args.environment.is_some() && !deploy),
        (
            "limit",
            args.limit.is_some() && operation == Operation::PullRequest,
        ),
    ]
    .into_iter()
    .filter_map(|(name, misplaced)| misplaced.then_some(name))
    .collect()
}

impl GithubReadArgs {
    /// The checked request; `repo` is resolved by the caller.
    pub fn checked(&self) -> Result<GithubRead, String> {
        let misplaced = unused(self, self.operation);
        if !misplaced.is_empty() {
            return Err(format!(
                "{} not used by this operation.",
                misplaced.join(", ")
            ));
        }
        match self.operation {
            Operation::PullRequests => {
                let state = self.state.unwrap_or_default();
                let since = checked(self.since.as_ref(), check_time, "since")?;
                let until = checked(self.until.as_ref(), check_time, "until")?;
                if let (Some(since), Some(until)) = (&since, &until) {
                    let (from, to) = (instant(since), instant(until));
                    if from.zip(to).is_some_and(|(from, to)| from > to) {
                        return Err("`since` is after `until`.".to_string());
                    }
                }
                Ok(GithubRead::PullRequests(ListQuery {
                    state,
                    window: window(state, since, until),
                    author: checked(self.author.as_ref(), check_login, "author")?,
                    base: checked(self.base.as_ref(), check_branch, "base")?,
                    search: checked(self.search.as_ref(), check_search, "search")?,
                    approved_by_me: self.approved_by_me.unwrap_or(false),
                    limit: check_limit(self.limit, DEFAULT_LIST_LIMIT)?,
                }))
            }
            Operation::PullRequest => {
                let number = self
                    .number
                    .ok_or_else(|| "pull_request needs `number`.".to_string())?;
                Ok(GithubRead::PullRequest(check_number(number)?))
            }
            Operation::Deployments => {
                let sha = checked(self.sha.as_ref(), check_sha, "sha")?;
                let target = match (sha, self.number) {
                    (Some(_), Some(_)) => return Err("Pass `sha` or `number`, not both.".into()),
                    (Some(sha), None) => Some(DeployTarget::Sha(sha)),
                    (None, Some(number)) => Some(DeployTarget::PullRequest(check_number(number)?)),
                    (None, None) => None,
                };
                Ok(GithubRead::Deployments(DeployQuery {
                    target,
                    environment: checked(
                        self.environment.as_ref(),
                        check_environment,
                        "environment",
                    )?,
                    limit: check_limit(self.limit, DEFAULT_DEPLOY_LIMIT)?,
                }))
            }
        }
    }
}

/// A checked time as an instant to order two of them (a date: its start, UTC).
fn instant(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|moment| moment.timestamp())
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|moment| moment.and_utc().timestamp())
        })
}

fn owned(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

pub fn list_argv(repo: &RepoSlug, query: &ListQuery) -> Vec<String> {
    let limit = query.limit.to_string();
    let fields = if query.approved_by_me {
        format!("{LIST_FIELDS},latestReviews")
    } else {
        LIST_FIELDS.to_string()
    };
    let mut argv = owned(&[
        "pr",
        "list",
        "--repo",
        &repo.full(),
        "--state",
        query.state.as_str(),
        "--limit",
        &limit,
        "--json",
        &fields,
    ]);
    if let Some(author) = &query.author {
        argv.push(format!("--author={author}"));
    }
    if let Some(base) = &query.base {
        argv.push(format!("--base={base}"));
    }
    let search: Vec<&str> = [
        query.window.as_deref(),
        query.approved_by_me.then_some("reviewed-by:@me"),
        query.search.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !search.is_empty() {
        argv.push(format!("--search={}", search.join(" ")));
    }
    argv
}

pub fn view_argv(repo: &RepoSlug, number: u64) -> Vec<String> {
    owned(&[
        "pr",
        "view",
        &number.to_string(),
        "--repo",
        &repo.full(),
        "--json",
        VIEW_FIELDS,
    ])
}

/// A GET on one of the fixed REST paths below, never another method.
fn api_argv(host: &str, path: String) -> Vec<String> {
    owned(&[
        "api",
        "--method",
        "GET",
        "--hostname",
        host,
        "-H",
        ACCEPT,
        &path,
    ])
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

pub fn deployments_argv(
    host: &str,
    repo: &RepoSlug,
    sha: Option<&str>,
    environment: Option<&str>,
    limit: u32,
) -> Vec<String> {
    let mut path = format!("repos/{}/deployments?per_page={limit}", repo.full());
    if let Some(sha) = sha {
        path.push_str(&format!("&sha={sha}"));
    }
    if let Some(environment) = environment {
        path.push_str(&format!("&environment={}", encode(environment)));
    }
    api_argv(host, path)
}

pub fn deployment_status_argv(host: &str, repo: &RepoSlug, id: u64) -> Vec<String> {
    api_argv(
        host,
        format!("repos/{}/deployments/{id}/statuses?per_page=1", repo.full()),
    )
}

pub fn commit_status_argv(host: &str, repo: &RepoSlug, sha: &str) -> Vec<String> {
    api_argv(host, format!("repos/{}/commits/{sha}/status", repo.full()))
}

/// Whether `head` holds `base` (GitHub's compare: `ahead` or `identical`).
pub fn compare_argv(host: &str, repo: &RepoSlug, base: &str, head: &str) -> Vec<String> {
    api_argv(
        host,
        format!("repos/{}/compare/{base}...{head}", repo.full()),
    )
}

pub fn viewer_argv(host: &str) -> Vec<String> {
    api_argv(host, "user".to_string())
}

/// The last check before running: only the shapes built above pass.
pub fn is_read_only_argv(argv: &[String]) -> bool {
    let forbidden = [
        "-X",
        "--method=",
        "-f",
        "-F",
        "--field",
        "--raw-field",
        "--input",
        "--paginate",
    ];
    let flags_ok = !argv.iter().any(|arg| {
        forbidden
            .iter()
            .any(|flag| arg == flag || (flag.ends_with('=') && arg.starts_with(flag)))
    });
    let shape_ok = match argv.first().map(String::as_str) {
        Some("pr") => matches!(argv.get(1).map(String::as_str), Some("list" | "view")),
        Some("api") => {
            argv.get(1..3) == Some(&["--method".to_string(), "GET".to_string()][..])
                && argv.iter().filter(|arg| *arg == "--method").count() == 1
                && argv
                    .last()
                    .is_some_and(|path| path == "user" || path.starts_with("repos/"))
        }
        _ => false,
    };
    flags_ok && shape_ok
}

#[cfg(test)]
#[path = "github_read_tests.rs"]
mod tests;
