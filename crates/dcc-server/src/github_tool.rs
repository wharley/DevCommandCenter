//! `dcc_github`: a resident agent reads GitHub through the `gh` the person is
//! signed into in DCC — pull requests, one pull request's reviews and checks,
//! a commit's deployments — so it can answer "what was merged and did it
//! reach production" without a token of its own or a task. Read only: the
//! calls are built and checked in `dcc_infra::forge::github_read`.

use std::time::Duration;

use dcc_core::domain::session::SessionId;
use dcc_infra::forge::github_read::{GithubReadArgs, GITHUB_TOOL, MAX_LIMIT};
use dcc_infra::forge::github_read_run::run_github_read;
use dcc_tauri::state::SessionCommandState;
use serde_json::{json, Value};

use crate::github_tool_repo::resolve_target;

/// How long the bridge holds a call open: a read runs `gh` a few times
/// (a deployment check up to about a dozen), each with its own timeout.
const RPC_TIMEOUT: Duration = Duration::from_secs(150);

pub fn is_github_tool(name: &str) -> bool {
    name == GITHUB_TOOL
}

/// The bridge's timeout for a JSON-RPC request that calls this tool.
pub fn rpc_timeout(request: &Value) -> Option<Duration> {
    (request.get("method").and_then(Value::as_str) == Some("tools/call")
        && request.pointer("/params/name").and_then(Value::as_str) == Some(GITHUB_TOOL))
    .then_some(RPC_TIMEOUT)
}

pub fn definition() -> Value {
    json!({
        "name": GITHUB_TOOL,
        "description": "Read GitHub through DCC, with the gh account the person uses in DCC for that repository; it never writes to GitHub. operation pull_requests lists a repository's pull requests (state open, merged, closed or all; since/until filter on mergedAt for merged, closedAt for closed, else updatedAt; author, base branch, search words; approvedByMe keeps those the person approved). pull_request shows one (number): state, mergeable, review decision and each reviewer's latest review, checks with a summary, files changed, merge commit. deployments reads GitHub deployments and commit statuses (where Vercel and others report): for a pull request (number) its merge commit's deployments and, under reached, whether each environment's newest successful deployment holds the merge commit; or for a sha; or the repository's latest (environment narrows). Times: a date (2026-11-09) or a moment with the person's UTC offset (2026-11-09T11:00:00-03:00). repo is owner/name or a DCC project's name; left out in a project's room, that project's repository.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "operation": {"type": "string", "enum": ["pull_requests", "pull_request", "deployments"]},
                "repo": {"type": "string", "maxLength": 200, "description": "owner/name, or a DCC project's name or id."},
                "state": {"type": "string", "enum": ["open", "merged", "closed", "all"], "default": "open"},
                "since": {"type": "string", "maxLength": 40},
                "until": {"type": "string", "maxLength": 40},
                "author": {"type": "string", "maxLength": 43, "description": "A login, app/name or @me."},
                "base": {"type": "string", "maxLength": 200, "description": "Base branch, e.g. main."},
                "search": {"type": "string", "maxLength": 256, "description": "Extra GitHub search words or qualifiers (not repo:, org:, user:)."},
                "approvedByMe": {"type": "boolean"},
                "number": {"type": "integer", "minimum": 1},
                "sha": {"type": "string", "pattern": "^[0-9a-fA-F]{7,40}$"},
                "environment": {"type": "string", "maxLength": 64, "description": "Deployment environment, e.g. Production."},
                "limit": {"type": "integer", "minimum": 1, "maximum": MAX_LIMIT}
            },
            "required": ["operation"]
        },
        "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true}
    })
}

fn parse(arguments: Option<Value>) -> Result<GithubReadArgs, String> {
    let arguments = arguments.ok_or_else(|| "Pass `operation`.".to_string())?;
    serde_json::from_value::<GithubReadArgs>(arguments).map_err(|error| error.to_string())
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    parse(arguments.clone()).is_ok_and(|args| args.checked().is_ok())
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

pub async fn dispatch(
    sessions: &SessionCommandState,
    session_id: &str,
    arguments: Option<Value>,
) -> Value {
    let args = match parse(arguments) {
        Ok(args) => args,
        Err(error) => return text(error, true),
    };
    let read = match args.checked() {
        Ok(read) => read,
        Err(error) => return text(error, true),
    };
    // In a project's room, that project's repository by default.
    let room = sessions
        .session_room(&SessionId(session_id.to_string()))
        .ok()
        .flatten();
    let db_path = sessions.database_file().to_path_buf();
    let repo = args.repo.clone();
    let answer = tokio::task::spawn_blocking(move || {
        let target = resolve_target(&db_path, repo.as_deref(), room.as_deref())?;
        run_github_read(&target, &read)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);
    match answer {
        Ok(answer) => text(
            serde_json::to_string_pretty(&answer).unwrap_or_default(),
            false,
        ),
        Err(error) => text(format!("GitHub read failed: {error}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_checked_reads_are_well_formed() {
        assert_eq!(definition()["name"], GITHUB_TOOL);
        assert_eq!(definition()["annotations"]["readOnlyHint"], true);
        assert!(call_is_well_formed(&Some(json!({
            "operation": "pull_requests", "repo": "acme/app", "state": "merged",
            "since": "2026-11-09T11:00:00-03:00", "base": "main"
        }))));
        assert!(call_is_well_formed(&Some(
            json!({"operation": "deployments", "number": 12})
        )));
        for bad in [
            None,
            Some(json!({})),
            Some(json!({"operation": "pull_request"})),
            Some(json!({"operation": "pull_requests", "limit": 500})),
            Some(json!({"operation": "pull_requests", "path": "repos/a/b"})),
        ] {
            assert!(!call_is_well_formed(&bad), "{bad:?}");
        }
        let call = json!({"method": "tools/call", "params": {"name": GITHUB_TOOL}});
        assert_eq!(rpc_timeout(&call), Some(RPC_TIMEOUT));
        assert_eq!(rpc_timeout(&json!({"method": "tools/list"})), None);
    }
}
