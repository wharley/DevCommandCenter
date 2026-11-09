//! `dcc_review_pull_request`: a resident agent asks DCC to review pull
//! requests (`dcc_core::domain::pr_review_request`) — the hub's own review,
//! run by the app, shown working in the team room, its drafts kept in Pull
//! Requests. No task is opened and nothing is published. A session that
//! runs as no agent gets an error.

use dcc_core::domain::pr_review_request::{
    parse_pull_request_url, MAX_REVIEWS_PER_CALL, REVIEW_PR_TOOL,
};
use dcc_core::domain::session::SessionId;
use dcc_tauri::state::SessionCommandState;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewArgs {
    urls: Vec<String>,
}

impl ReviewArgs {
    fn checked(self) -> Option<Vec<String>> {
        let fits = (1..=MAX_REVIEWS_PER_CALL).contains(&self.urls.len())
            && self
                .urls
                .iter()
                .all(|url| parse_pull_request_url(url).is_some());
        fits.then_some(self.urls)
    }
}

pub fn is_review_tool(name: &str) -> bool {
    name == REVIEW_PR_TOOL
}

pub fn definition() -> Value {
    json!({
        "name": REVIEW_PR_TOOL,
        "description": "Only when you run as a DCC resident agent: review pull requests the person names, links or asks you to find. DCC runs the same review as Pull Requests › Gerar revisão in the person's checkout of each repository: the team room shows it working and, when done, its draft comments wait in Pull Requests for the person to send. Nothing is published and no task is opened — use this instead of opening a task to review. Tell the person in a line which pull requests went and stop; the results arrive in the room.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "urls": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_REVIEWS_PER_CALL,
                    "items": {"type": "string", "pattern": "^https://[^/]+/[^/]+/[^/]+/pull/[0-9]+", "maxLength": 512},
                    "description": "Each pull request's link, as https://github.com/owner/name/pull/123."
                }
            },
            "required": ["urls"]
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn call_is_well_formed(arguments: &Option<Value>) -> bool {
    arguments.as_ref().is_some_and(|arguments| {
        serde_json::from_value::<ReviewArgs>(arguments.clone())
            .ok()
            .and_then(ReviewArgs::checked)
            .is_some()
    })
}

fn text(message: String, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": is_error})
}

pub fn dispatch(
    sessions: &SessionCommandState,
    session_id: &str,
    arguments: Option<Value>,
) -> Value {
    let Some(urls) = arguments
        .and_then(|arguments| serde_json::from_value::<ReviewArgs>(arguments).ok())
        .and_then(ReviewArgs::checked)
    else {
        return text(
            format!("Pass `urls`: 1 to {MAX_REVIEWS_PER_CALL} pull request links, as https://github.com/owner/name/pull/123."),
            true,
        );
    };
    match sessions.request_pr_reviews_for_session(&SessionId(session_id.to_string()), &urls) {
        Ok((_, added)) => {
            let already = urls.len() - added;
            let note = if already > 0 {
                format!(" {already} of them was already being reviewed.")
            } else {
                String::new()
            };
            text(
                format!(
                    "DCC is reviewing {} pull request(s) now, as Pull Requests › Gerar revisão does; the team room shows each one working and then its result, and the drafts wait in Pull Requests.{note} Tell the person in a line which ones went and stop; do not open a task for it.",
                    urls.len()
                ),
                false,
            )
        }
        Err(error) => text(error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_needs_one_to_ten_pull_request_links() {
        let good = Some(json!({"urls": ["https://github.com/acme/app/pull/7"]}));
        assert!(call_is_well_formed(&good));
        let many: Vec<String> = (1..=11)
            .map(|number| format!("https://github.com/acme/app/pull/{number}"))
            .collect();
        for bad in [
            json!({"urls": []}),
            json!({"urls": ["https://github.com/acme/app/issues/7"]}),
            json!({"urls": many}),
            json!({"urls": ["https://github.com/acme/app/pull/7"], "extra": 1}),
        ] {
            assert!(!call_is_well_formed(&Some(bad.clone())), "{bad}");
        }
        assert_eq!(definition()["name"], REVIEW_PR_TOOL);
    }
}
