//! Runs a checked `dcc_github` read (`github_read`) through the person's
//! `gh`, on the account DCC uses for the repository, each call with a
//! timeout, and shapes GitHub's answer into the few fields an agent needs,
//! capped so one answer stays small.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::forge::github::resolve_auth_context;
use crate::forge::github_read::{
    commit_status_argv, compare_argv, deployment_status_argv, deployments_argv, is_read_only_argv,
    list_argv, view_argv, viewer_argv, DeployQuery, DeployTarget, GithubRead, ListQuery, RepoSlug,
};
use crate::forge::resolve_cli_binary;
use crate::process::run_command_with_timeout;

const GH_READ_TIMEOUT: Duration = Duration::from_secs(20);
/// The answer's size the items are cut to.
pub const MAX_ANSWER_CHARS: usize = 60_000;
const MAX_CHECKS: usize = 40;
/// Deployments whose latest status is read (one call each).
const MAX_STATUS_READS: usize = 6;
/// Environments whose newest success is compared with a merge commit.
const MAX_COMPARES: usize = 3;

/// Where a read goes: the repository's host and the account DCC uses for it
/// (`None`: the CLI's active account).
#[derive(Clone, Debug)]
pub struct GithubTarget {
    pub host: String,
    pub repo: RepoSlug,
    pub login: Option<String>,
}

struct Gh {
    program: std::path::PathBuf,
    host: String,
    envs: Vec<(String, String)>,
}

impl Gh {
    fn new(target: &GithubTarget) -> Result<Self, String> {
        let auth = resolve_auth_context(&target.host, target.login.as_deref())?;
        Ok(Self {
            program: resolve_cli_binary("gh")?,
            host: target.host.clone(),
            envs: auth.map(|auth| auth.envs).unwrap_or_default(),
        })
    }

    fn json(&self, argv: &[String]) -> Result<Value, String> {
        if !is_read_only_argv(argv) {
            return Err("DCC refused a GitHub call that is not a read.".to_string());
        }
        let output = run_command_with_timeout(
            &self.program,
            |command| {
                command
                    .current_dir(std::env::temp_dir())
                    .args(argv)
                    .env("GH_HOST", &self.host)
                    .envs(self.envs.iter().map(|(key, value)| (key, value)));
            },
            GH_READ_TIMEOUT,
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr: String = stderr.trim().chars().take(500).collect();
            return Err(if stderr.is_empty() {
                "GitHub CLI request failed.".to_string()
            } else {
                stderr
            });
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("Failed to decode GitHub's answer: {error}"))
    }
}

/// Runs `read` against `target`; the answer is JSON for the agent.
pub fn run_github_read(target: &GithubTarget, read: &GithubRead) -> Result<Value, String> {
    let gh = Gh::new(target)?;
    let mut answer = match read {
        GithubRead::PullRequests(query) => pull_requests(&gh, target, query)?,
        GithubRead::PullRequest(number) => {
            pull_request_view(&gh.json(&view_argv(&target.repo, *number))?)
        }
        GithubRead::Deployments(query) => deployments(&gh, target, query)?,
    };
    answer["repository"] = json!(target.repo.full());
    answer["account"] = json!(target
        .login
        .clone()
        .unwrap_or_else(|| "gh's active account".to_string()));
    Ok(answer)
}

fn pull_requests(gh: &Gh, target: &GithubTarget, query: &ListQuery) -> Result<Value, String> {
    let raw = gh.json(&list_argv(&target.repo, query))?;
    let viewer = if query.approved_by_me {
        Some(match &target.login {
            Some(login) => login.clone(),
            None => str_at(&gh.json(&viewer_argv(&gh.host))?, "/login")
                .ok_or_else(|| "Could not tell which GitHub account is signed in.".to_string())?,
        })
    } else {
        None
    };
    let items = list_items(&raw, viewer.as_deref());
    let (items, cut) = fit(items, MAX_ANSWER_CHARS);
    let mut answer = json!({"pullRequests": items});
    note_cut(&mut answer, cut);
    if let Some(viewer) = viewer {
        answer["approvedBy"] = json!(viewer);
    }
    Ok(answer)
}

fn str_at(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Keeps the fields that are set.
fn compact(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(
        pairs
            .into_iter()
            .filter(|(_, value)| !value.is_null() && value != "")
            .map(|(key, value)| (key.to_string(), value))
            .collect::<Map<_, _>>(),
    )
}

fn field(raw: &Value, key: &str) -> Value {
    raw.get(key).cloned().unwrap_or(Value::Null)
}

fn login_at(raw: &Value, pointer: &str) -> Value {
    str_at(raw, pointer).map(Value::from).unwrap_or(Value::Null)
}

/// `gh pr list`'s rows, those `viewer` approved when given.
pub fn list_items(raw: &Value, viewer: Option<&str>) -> Vec<Value> {
    raw.as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|pr| viewer.is_none_or(|viewer| approved_by(pr, viewer)))
        .map(|pr| {
            compact(vec![
                ("number", field(pr, "number")),
                ("title", field(pr, "title")),
                ("url", field(pr, "url")),
                ("state", field(pr, "state")),
                ("isDraft", field(pr, "isDraft")),
                ("author", login_at(pr, "/author/login")),
                ("base", field(pr, "baseRefName")),
                ("head", field(pr, "headRefName")),
                ("createdAt", field(pr, "createdAt")),
                ("updatedAt", field(pr, "updatedAt")),
                ("mergedAt", field(pr, "mergedAt")),
                ("mergedBy", login_at(pr, "/mergedBy/login")),
                ("mergeCommit", login_at(pr, "/mergeCommit/oid")),
                ("closedAt", field(pr, "closedAt")),
                ("reviewDecision", field(pr, "reviewDecision")),
            ])
        })
        .collect()
}

fn approved_by(pr: &Value, viewer: &str) -> bool {
    pr.get("latestReviews")
        .and_then(Value::as_array)
        .is_some_and(|reviews| {
            reviews.iter().any(|review| {
                review.get("state").and_then(Value::as_str) == Some("APPROVED")
                    && str_at(review, "/author/login")
                        .is_some_and(|login| login.eq_ignore_ascii_case(viewer))
            })
        })
}

/// One check of the rollup: a check run or a commit status.
fn check(raw: &Value) -> Value {
    let name = str_at(raw, "/name").or_else(|| str_at(raw, "/context"));
    let result = str_at(raw, "/conclusion")
        .filter(|value| !value.is_empty())
        .or_else(|| str_at(raw, "/state"))
        .or_else(|| str_at(raw, "/status"));
    compact(vec![
        ("name", json!(name)),
        ("workflow", field(raw, "workflowName")),
        ("result", json!(result)),
    ])
}

fn check_bucket(result: &str) -> &'static str {
    match result.to_ascii_uppercase().as_str() {
        "SUCCESS" | "NEUTRAL" | "SKIPPED" => "passed",
        "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
            "failed"
        }
        _ => "pending",
    }
}

/// `gh pr view`'s answer, reviews and checks reduced to who and how.
pub fn pull_request_view(raw: &Value) -> Value {
    let reviews: Vec<Value> = raw
        .get("latestReviews")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|review| {
            compact(vec![
                ("author", login_at(review, "/author/login")),
                ("state", field(review, "state")),
                ("submittedAt", field(review, "submittedAt")),
            ])
        })
        .collect();
    let checks: Vec<Value> = raw
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(check)
        .collect();
    let mut summary = Map::new();
    for check in &checks {
        let bucket = check_bucket(check["result"].as_str().unwrap_or_default());
        let count = summary.entry(bucket).or_insert(json!(0));
        *count = json!(count.as_u64().unwrap_or(0) + 1);
    }
    let shown = checks.len().min(MAX_CHECKS);
    let mut view = compact(vec![
        ("number", field(raw, "number")),
        ("title", field(raw, "title")),
        ("url", field(raw, "url")),
        ("state", field(raw, "state")),
        ("isDraft", field(raw, "isDraft")),
        ("author", login_at(raw, "/author/login")),
        ("base", field(raw, "baseRefName")),
        ("head", field(raw, "headRefName")),
        ("headSha", field(raw, "headRefOid")),
        ("mergeable", field(raw, "mergeable")),
        ("mergeState", field(raw, "mergeStateStatus")),
        ("reviewDecision", field(raw, "reviewDecision")),
        ("changedFiles", field(raw, "changedFiles")),
        ("additions", field(raw, "additions")),
        ("deletions", field(raw, "deletions")),
        ("createdAt", field(raw, "createdAt")),
        ("updatedAt", field(raw, "updatedAt")),
        ("mergedAt", field(raw, "mergedAt")),
        ("mergedBy", login_at(raw, "/mergedBy/login")),
        ("mergeCommit", login_at(raw, "/mergeCommit/oid")),
        ("closedAt", field(raw, "closedAt")),
    ]);
    view["reviews"] = json!(reviews);
    view["checksSummary"] = Value::Object(summary);
    view["checks"] = json!(checks[..shown]);
    if shown < checks.len() {
        view["checksCut"] = json!(format!("{shown} of {} checks shown", checks.len()));
    }
    view
}

fn deployment(raw: &Value, status: Option<&Value>) -> Value {
    let mut view = compact(vec![
        ("id", field(raw, "id")),
        ("environment", field(raw, "environment")),
        ("sha", field(raw, "sha")),
        ("ref", field(raw, "ref")),
        ("createdAt", field(raw, "created_at")),
        ("creator", login_at(raw, "/creator/login")),
    ]);
    if let Some(status) = status {
        view["latestStatus"] = compact(vec![
            ("state", field(status, "state")),
            ("at", field(status, "created_at")),
            ("description", field(status, "description")),
            ("url", field(status, "environment_url")),
        ]);
    }
    view
}

/// Deployments with the latest status of the first few.
fn read_deployments(gh: &Gh, repo: &RepoSlug, argv: &[String]) -> Result<Vec<Value>, String> {
    let raw = gh.json(argv)?;
    Ok(raw
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, raw)| {
            let status = (index < MAX_STATUS_READS)
                .then(|| raw.get("id").and_then(Value::as_u64))
                .flatten()
                .and_then(|id| gh.json(&deployment_status_argv(&gh.host, repo, id)).ok())
                .and_then(|statuses| statuses.as_array().and_then(|all| all.first().cloned()));
            deployment(raw, status.as_ref())
        })
        .collect())
}

fn commit_status(raw: &Value) -> Value {
    let statuses: Vec<Value> = raw
        .get("statuses")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .take(MAX_CHECKS)
        .map(|status| {
            compact(vec![
                ("context", field(status, "context")),
                ("state", field(status, "state")),
                ("description", field(status, "description")),
                ("url", field(status, "target_url")),
                ("updatedAt", field(status, "updated_at")),
            ])
        })
        .collect();
    json!({"state": field(raw, "state"), "statuses": statuses})
}

/// The newest successful deployment of each environment, at most a few.
pub fn newest_successes(deployments: &[Value]) -> Vec<(String, String)> {
    let mut picked: Vec<(String, String)> = Vec::new();
    for deployment in deployments {
        let (Some(environment), Some(sha)) = (
            str_at(deployment, "/environment"),
            str_at(deployment, "/sha"),
        ) else {
            continue;
        };
        let success = str_at(deployment, "/latestStatus/state").as_deref() == Some("success");
        if success && !picked.iter().any(|(known, _)| *known == environment) {
            picked.push((environment, sha));
        }
    }
    picked.truncate(MAX_COMPARES);
    picked
}

fn deployments(gh: &Gh, target: &GithubTarget, query: &DeployQuery) -> Result<Value, String> {
    let repo = &target.repo;
    let environment = query.environment.as_deref();
    let mut answer = json!({});
    let sha = match &query.target {
        Some(DeployTarget::Sha(sha)) => Some(sha.clone()),
        Some(DeployTarget::PullRequest(number)) => {
            let pr = pull_request_view(&gh.json(&view_argv(repo, *number))?);
            let merge = str_at(&pr, "/mergeCommit");
            answer["pullRequest"] = compact(vec![
                ("number", field(&pr, "number")),
                ("state", field(&pr, "state")),
                ("mergedAt", field(&pr, "mergedAt")),
                ("mergeCommit", json!(merge)),
                ("headSha", field(&pr, "headSha")),
            ]);
            merge.or_else(|| str_at(&pr, "/headSha"))
        }
        None => None,
    };
    if let Some(sha) = &sha {
        answer["sha"] = json!(sha);
        let argv = deployments_argv(&gh.host, repo, Some(sha), environment, query.limit);
        answer["deploymentsOfSha"] = json!(read_deployments(gh, repo, &argv)?);
        answer["commitStatus"] = commit_status(&gh.json(&commit_status_argv(&gh.host, repo, sha))?);
    }
    let argv = deployments_argv(&gh.host, repo, None, environment, query.limit);
    let latest = read_deployments(gh, repo, &argv)?;
    // A merged pull request reached an environment when its newest
    // successful deployment holds the merge commit.
    let merged = answer
        .pointer("/pullRequest/mergeCommit")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(merge) = merged {
        let reached: Vec<Value> = newest_successes(&latest)
            .into_iter()
            .map(|(environment, deployed)| {
                let status = gh
                    .json(&compare_argv(&gh.host, repo, &merge, &deployed))
                    .ok()
                    .and_then(|compare| str_at(&compare, "/status"));
                let contains = status
                    .as_deref()
                    .map(|status| matches!(status, "ahead" | "identical"));
                json!({"environment": environment, "deployedSha": deployed, "containsMergeCommit": contains})
            })
            .collect();
        answer["reached"] = json!(reached);
    }
    let (latest, cut) = fit(latest, MAX_ANSWER_CHARS / 2);
    answer["latestDeployments"] = json!(latest);
    note_cut(&mut answer, cut);
    Ok(answer)
}

/// The leading items that fit in `max_chars`, and how many were left out.
pub fn fit(mut items: Vec<Value>, max_chars: usize) -> (Vec<Value>, usize) {
    let total = items.len();
    let mut used = 2;
    let mut kept = 0;
    for item in &items {
        used += item.to_string().len() + 1;
        if used > max_chars {
            break;
        }
        kept += 1;
    }
    items.truncate(kept);
    (items, total - kept)
}

fn note_cut(answer: &mut Value, cut: usize) {
    if cut > 0 {
        answer["truncated"] = json!(format!(
            "{cut} more left out to keep the answer short; narrow the query (limit, since, search)."
        ));
    }
}

#[cfg(test)]
#[path = "github_read_run_tests.rs"]
mod tests;
