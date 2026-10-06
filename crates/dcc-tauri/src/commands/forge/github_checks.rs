//! Failing GitHub checks of a task's PR, turned into the delivery failure the
//! Inspector shows and can send to the agent — the same path GitLab pipelines
//! already take. Logs are fetched once per commit and set of failed jobs.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Mutex, OnceLock},
};

use serde_json::Value;

/// Logs fetched per refresh; the agent can pull the rest with the command
/// printed next to each job.
const MAX_JOB_LOGS: usize = 3;
const EXCERPT_MAX_LINES: usize = 40;
const EXCERPT_MAX_BYTES: usize = 1_800;
/// Last lines of the step, kept for the summary tools print at the end.
const EXCERPT_TAIL_LINES: usize = 3;
/// Line markers of a real failure. Plain "error" is too loose: linters print
/// it in advice ("passes a yieldable error value").
const ERROR_MARKERS: &[&str] = &[
    "error TS", "error:", "error[", "Error:", "ERROR", "FAIL", "panicked", "failed:", "✗", "×",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FailedCheck {
    pub name: String,
    pub workflow: Option<String>,
    pub link: Option<String>,
    /// GitHub Actions job id, when the check is an Actions job.
    pub job_id: Option<u64>,
}

/// Checks from `gh pr checks --json name,bucket,link,workflow` that failed.
pub(crate) fn failed_checks(checks: &Value) -> Vec<FailedCheck> {
    checks
        .as_array()
        .into_iter()
        .flatten()
        .filter(|check| check.get("bucket").and_then(Value::as_str) == Some("fail"))
        .map(|check| {
            let text = |key: &str| {
                check
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
            };
            let link = text("link");
            FailedCheck {
                name: text("name").unwrap_or_else(|| "Check".to_string()),
                workflow: text("workflow"),
                job_id: link.as_deref().and_then(actions_job_id),
                link,
            }
        })
        .collect()
}

/// Job id from an Actions link: `…/actions/runs/<run>/job/<job>`.
pub(crate) fn actions_job_id(link: &str) -> Option<u64> {
    let (_, rest) = link.split_once("/actions/runs/")?;
    let (_, job) = rest.split_once("/job/")?;
    job.split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Jobs whose logs this refresh fetches.
pub(crate) fn jobs_to_fetch(checks: &[FailedCheck]) -> Vec<u64> {
    checks
        .iter()
        .filter_map(|check| check.job_id)
        .take(MAX_JOB_LOGS)
        .collect()
}

/// What matters in the step that failed, from a `gh run view --log-failed`
/// output.
///
/// Each line is `job<TAB>step<TAB>timestamp message`. The failing step runs
/// from the last `##[group]Run …` before the first `##[error]` up to that
/// error; what follows is runner cleanup and only adds noise. Inside the step,
/// the first error lines win (the root cause usually comes first; warnings
/// pile up at the end), plus the last lines for the tool's summary. With no
/// recognizable error line, it is the end of the step. Skipped lines show
/// as `…`.
pub(crate) fn failing_step_excerpt(log: &str) -> String {
    let lines: Vec<&str> = log.lines().map(log_message).collect();
    if lines.is_empty() {
        return String::new();
    }
    let error_at = lines.iter().position(|line| line.starts_with("##[error]"));
    let end = error_at.map_or(lines.len(), |index| index + 1);
    let start = lines[..end]
        .iter()
        .rposition(|line| line.starts_with("##[group]Run "))
        .unwrap_or(0);
    let step = lines[start]
        .strip_prefix("##[group]")
        .filter(|_| lines[start].starts_with("##[group]Run "));
    let body: Vec<&str> = lines[start..end]
        .iter()
        .copied()
        .filter(|line| !line.starts_with("##[group]") && !line.starts_with("##[endgroup]"))
        .collect();

    let tail_from = body.len().saturating_sub(EXCERPT_TAIL_LINES);
    let mut picked: BTreeSet<usize> = (tail_from..body.len()).collect();
    let mut bytes: usize = picked.iter().map(|&index| body[index].len() + 1).sum();
    let errors: Vec<usize> = (0..tail_from)
        .filter(|&index| {
            ERROR_MARKERS
                .iter()
                .any(|marker| body[index].contains(marker))
        })
        .collect();
    let candidates: Vec<usize> = if errors.is_empty() {
        (0..tail_from).rev().collect()
    } else {
        errors
    };
    for index in candidates {
        let size = body[index].len() + 1;
        if picked.len() == EXCERPT_MAX_LINES || bytes + size > EXCERPT_MAX_BYTES {
            break;
        }
        bytes += size;
        picked.insert(index);
    }

    let mut out: Vec<&str> = Vec::new();
    if let Some(step) = step {
        out.push(step.trim_start_matches("Run ").trim());
    }
    let mut next = 0;
    for index in picked {
        if index > next {
            out.push("…");
        }
        out.push(body[index]);
        next = index + 1;
    }
    let mut excerpt = out.join("\n");
    if step.is_some() {
        excerpt.insert_str(0, "$ ");
    }
    excerpt
}

fn log_message(line: &str) -> &str {
    let message = line.splitn(3, '\t').nth(2).unwrap_or(line);
    // Drop the runner's ISO timestamp: `2026-10-06T10:30:40.5656418Z `.
    match message.split_once(' ') {
        Some((stamp, rest))
            if stamp.len() >= 20
                && stamp.ends_with('Z')
                && stamp.as_bytes().get(4) == Some(&b'-')
                && stamp.as_bytes().get(10) == Some(&b'T') =>
        {
            rest
        }
        _ => message,
    }
}

/// The text captured as the pipeline delivery failure. "Failed jobs:" is what
/// classifies it as a pipeline/job failure.
pub(crate) fn failed_checks_detail(
    pr_number: Option<u64>,
    head_sha: Option<&str>,
    checks: &[FailedCheck],
    logs: &HashMap<u64, String>,
) -> String {
    let pr = pr_number.map_or_else(|| "the PR".to_string(), |number| format!("PR #{number}"));
    let mut detail = match head_sha {
        Some(sha) => format!(
            "GitHub checks failed for {pr} at commit {}.",
            &sha[..sha.len().min(12)]
        ),
        None => format!("GitHub checks failed for {pr}."),
    };
    detail.push_str("\nFailed jobs:");
    for check in checks {
        let label = match &check.workflow {
            Some(workflow) => format!("{workflow} / {}", check.name),
            None => check.name.clone(),
        };
        match &check.link {
            Some(link) => detail.push_str(&format!("\n- {label}: {link}")),
            None => detail.push_str(&format!("\n- {label}")),
        }
    }
    for check in checks {
        let Some(job_id) = check.job_id else { continue };
        let Some(log) = logs.get(&job_id) else {
            continue;
        };
        detail.push_str(&format!(
            "\n\n{} — end of the failing step (full log: gh run view --job {job_id} --log-failed):\n{}",
            check.name,
            failing_step_excerpt(log)
        ));
    }
    detail
}

/// Same commit and same failed jobs means the same logs.
pub(crate) fn detail_cache_key(head_sha: Option<&str>, checks: &[FailedCheck]) -> String {
    let mut jobs: Vec<String> = checks
        .iter()
        .map(|check| check.link.clone().unwrap_or_else(|| check.name.clone()))
        .collect();
    jobs.sort();
    format!("{}\u{1f}{}", head_sha.unwrap_or(""), jobs.join("\u{1f}"))
}

fn detail_cache() -> &'static Mutex<HashMap<String, (String, String)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Detail already built for this task root and key, if any.
pub(crate) fn cached_detail(root: &str, key: &str) -> Option<String> {
    let cache = detail_cache().lock().ok()?;
    cache
        .get(root)
        .filter(|(cached_key, _)| cached_key == key)
        .map(|(_, detail)| detail.clone())
}

pub(crate) fn remember_detail(root: &str, key: String, detail: String) {
    if let Ok(mut cache) = detail_cache().lock() {
        cache.insert(root.to_string(), (key, detail));
    }
}

pub(crate) fn forget_detail(root: &str) {
    if let Ok(mut cache) = detail_cache().lock() {
        cache.remove(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_only_failed_checks_and_reads_the_actions_job() {
        let checks = json!([
            { "name": "Typecheck", "bucket": "fail", "workflow": "CI",
              "link": "https://github.com/o/r/actions/runs/37/job/112224375120" },
            { "name": "Lint", "bucket": "pass", "workflow": "CI", "link": "https://x" },
            { "name": "Vercel", "bucket": "fail", "workflow": "", "link": "https://vercel.com/o/r/1" },
            { "name": "Docs", "bucket": "pending" }
        ]);
        assert_eq!(
            failed_checks(&checks),
            vec![
                FailedCheck {
                    name: "Typecheck".to_string(),
                    workflow: Some("CI".to_string()),
                    link: Some(
                        "https://github.com/o/r/actions/runs/37/job/112224375120".to_string()
                    ),
                    job_id: Some(112224375120),
                },
                FailedCheck {
                    name: "Vercel".to_string(),
                    workflow: None,
                    link: Some("https://vercel.com/o/r/1".to_string()),
                    job_id: None,
                },
            ]
        );
    }

    #[test]
    fn job_id_ignores_query_and_fragments() {
        assert_eq!(
            actions_job_id("https://github.com/o/r/actions/runs/1/job/42?pr=7#step:3:1"),
            Some(42)
        );
        assert_eq!(
            actions_job_id("https://github.com/o/r/actions/runs/1"),
            None
        );
    }

    #[test]
    fn excerpt_is_the_failing_step_without_runner_cleanup() {
        let log = [
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:29:35.7181957Z ##[group]Initializing the repository",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:29:35.7181957Z git init",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:04.0579485Z ##[group]Run vpr typecheck",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:04.0579485Z vpr typecheck",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:04.0579485Z ##[endgroup]",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:40.4022883Z src/a.ts(1071,11): error TS2304: Cannot find name 'testLayer'.",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:40.5656418Z ##[error]Process completed with exit code 1.",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:40.5735195Z Post job cleanup.",
            "Typecheck\tUNKNOWN STEP\t2026-10-06T10:30:40.6832361Z Cleaning up orphan processes",
        ]
        .join("\n");
        assert_eq!(
            failing_step_excerpt(&log),
            "$ vpr typecheck\nvpr typecheck\nsrc/a.ts(1071,11): error TS2304: Cannot find name 'testLayer'.\n##[error]Process completed with exit code 1."
        );
    }

    #[test]
    fn excerpt_keeps_the_end_of_a_long_step() {
        let mut lines =
            vec!["j\ts\t2026-10-06T10:00:00.0000000Z ##[group]Run cargo test".to_string()];
        lines.extend((0..100).map(|i| format!("j\ts\t2026-10-06T10:00:00.0000000Z line {i}")));
        lines.push("j\ts\t2026-10-06T10:00:00.0000000Z ##[error]boom".to_string());
        let excerpt = failing_step_excerpt(&lines.join("\n"));
        assert!(excerpt.starts_with("$ cargo test\n…\n"));
        assert!(excerpt.ends_with("line 99\n##[error]boom"));
        assert!(!excerpt.contains("line 0\n"));
    }

    #[test]
    fn excerpt_prefers_the_first_errors_over_trailing_warnings() {
        let mut lines = vec!["j\ts\t2026-10-06T10:00:00.0000000Z ##[group]Run tsc".to_string()];
        lines.push(
            "j\ts\t2026-10-06T10:00:00.0000000Z src/a.ts(1,1): error TS2304: Cannot find name 'x'."
                .to_string(),
        );
        lines.extend((0..80).map(|i| {
            format!("j\ts\t2026-10-06T10:00:00.0000000Z src/w{i}.ts(1,1): suggestion: passes a yieldable error value")
        }));
        lines.push("j\ts\t2026-10-06T10:00:00.0000000Z 1 failed".to_string());
        lines.push(
            "j\ts\t2026-10-06T10:00:00.0000000Z ##[error]Process completed with exit code 1."
                .to_string(),
        );
        assert_eq!(
            failing_step_excerpt(&lines.join("\n")),
            "$ tsc\nsrc/a.ts(1,1): error TS2304: Cannot find name 'x'.\n…\n\
             src/w79.ts(1,1): suggestion: passes a yieldable error value\n1 failed\n\
             ##[error]Process completed with exit code 1."
        );
    }

    #[test]
    fn detail_lists_every_failure_and_logs_for_actions_jobs() {
        let checks = vec![
            FailedCheck {
                name: "Typecheck".to_string(),
                workflow: Some("CI".to_string()),
                link: Some("https://github.com/o/r/actions/runs/1/job/9".to_string()),
                job_id: Some(9),
            },
            FailedCheck {
                name: "Vercel".to_string(),
                workflow: None,
                link: None,
                job_id: None,
            },
        ];
        let logs = HashMap::from([(
            9,
            "j\ts\t2026-10-06T10:00:00.0000000Z ##[group]Run tsc\nj\ts\t2026-10-06T10:00:00.0000000Z ##[error]bad".to_string(),
        )]);
        assert_eq!(
            failed_checks_detail(Some(12), Some("abcdef1234567890"), &checks, &logs),
            "GitHub checks failed for PR #12 at commit abcdef123456.\n\
             Failed jobs:\n\
             - CI / Typecheck: https://github.com/o/r/actions/runs/1/job/9\n\
             - Vercel\n\n\
             Typecheck — end of the failing step (full log: gh run view --job 9 --log-failed):\n\
             $ tsc\n##[error]bad"
        );
    }

    #[test]
    fn cache_key_ignores_check_order_but_not_the_commit() {
        let a = FailedCheck {
            name: "a".into(),
            workflow: None,
            link: Some("1".into()),
            job_id: None,
        };
        let b = FailedCheck {
            name: "b".into(),
            workflow: None,
            link: Some("2".into()),
            job_id: None,
        };
        assert_eq!(
            detail_cache_key(Some("sha"), &[a.clone(), b.clone()]),
            detail_cache_key(Some("sha"), &[b.clone(), a.clone()])
        );
        assert_ne!(
            detail_cache_key(Some("sha"), &[a.clone()]),
            detail_cache_key(Some("other"), &[a])
        );
    }
}
