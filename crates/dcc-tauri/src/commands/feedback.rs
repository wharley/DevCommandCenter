//! Global DCC feedback. The destination is deliberately independent of workspace remotes.
use std::{path::Path, time::Duration};

use reqwest::{blocking::Client, header};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::forge::github;
use super::forge_commands::{resolve_forge_cli_snapshot, ForgeCliProvider};
use crate::state::WorkspaceCommandState;

const API: &str = "https://api.github.com";
const REPOSITORY: &str = "wharley/DevCommandCenter";
const PAGE_SIZE: usize = 30;
const UNCERTAIN: &str = "FEEDBACK_UNCERTAIN";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackContext {
    pub login: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
}

pub fn context(state: &WorkspaceCommandState, version: String) -> Result<FeedbackContext, String> {
    let status = resolve_forge_cli_snapshot(state, ForgeCliProvider::Github, "github.com", true)?;
    let login = status.selected_login.ok_or("FEEDBACK_AUTH_REQUIRED")?;
    Ok(FeedbackContext {
        login,
        version,
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeedbackInput {
    pub request_id: String,
    pub login: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub steps: String,
    pub expected: String,
    pub include_diagnostics: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackIssue {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub category: String,
    pub state: String,
    pub state_reason: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPage {
    pub issues: Vec<FeedbackIssue>,
    pub has_next: bool,
}

fn issue(value: &Value) -> Result<FeedbackIssue, String> {
    let number = value["number"].as_u64().ok_or("Invalid issue number")?;
    let title = value["title"]
        .as_str()
        .ok_or("Invalid issue title")?
        .to_string();
    let body = value["body"].as_str().unwrap_or_default();
    let category =
        if body.contains("<!-- dcc-feedback-category:bug -->") || title.starts_with("[Bug]") {
            "bug"
        } else if body.contains("<!-- dcc-feedback-category:improvement -->")
            || title.starts_with("[Feature]")
        {
            "improvement"
        } else {
            "other"
        };
    Ok(FeedbackIssue {
        number,
        title,
        // Never open an arbitrary URL supplied by issue content or a redirect.
        url: format!("https://github.com/{REPOSITORY}/issues/{number}"),
        category: category.into(),
        state: value["state"].as_str().ok_or("Invalid issue state")?.into(),
        state_reason: value["state_reason"].as_str().map(String::from),
        created_at: value["created_at"]
            .as_str()
            .ok_or("Invalid issue date")?
            .into(),
    })
}

fn client(login: &str) -> Result<Client, String> {
    let auth =
        github::resolve_auth_context("github.com", Some(login))?.ok_or("FEEDBACK_AUTH_REQUIRED")?;
    let token = auth
        .envs
        .iter()
        .find(|(key, _)| key == "GH_TOKEN")
        .map(|(_, token)| token)
        .ok_or("FEEDBACK_AUTH_REQUIRED")?;
    let mut authorization = header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| "Invalid GitHub credential")?;
    authorization.set_sensitive(true);
    let mut headers = header::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, authorization);
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/vnd.github+json"),
    );
    Client::builder()
        .default_headers(headers)
        .user_agent("DCC-Feedback")
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Could not initialize GitHub connection".into())
}

fn raw_page(client: &Client, login: &str, page: u32) -> Result<Vec<Value>, String> {
    let response = client
        .get(format!("{API}/repos/{REPOSITORY}/issues"))
        .query(&[
            ("creator", login),
            ("state", "all"),
            ("sort", "created"),
            ("direction", "desc"),
            ("per_page", &PAGE_SIZE.to_string()),
            ("page", &page.to_string()),
        ])
        .send()
        .map_err(|_| "FEEDBACK_NETWORK_ERROR")?;
    if !response.status().is_success() {
        return Err(format!("FEEDBACK_GITHUB_{}", response.status().as_u16()));
    }
    serde_json::from_str(&response.text().map_err(|_| "FEEDBACK_NETWORK_ERROR")?)
        .map_err(|_| "Invalid GitHub response".into())
}

pub fn list(login: &str, page: u32) -> Result<FeedbackPage, String> {
    if page == 0 || page > 10_000 {
        return Err("Invalid page".into());
    }
    let values = raw_page(&client(login)?, login, page)?;
    let has_next = values.len() == PAGE_SIZE;
    let issues = values
        .iter()
        .filter(|value| value.get("pull_request").is_none())
        .map(issue)
        .collect::<Result<_, _>>()?;
    Ok(FeedbackPage { issues, has_next })
}

fn format_submission(
    input: &FeedbackInput,
    context: &FeedbackContext,
) -> Result<(String, String), String> {
    uuid::Uuid::parse_str(&input.request_id).map_err(|_| "Invalid feedback request ID")?;
    if input.login != context.login {
        return Err("FEEDBACK_ACCOUNT_CHANGED".into());
    }
    let prefix = match input.category.as_str() {
        "bug" => "Bug",
        "improvement" => "Feature",
        "other" => "Feedback",
        _ => return Err("Invalid feedback category".into()),
    };
    for (value, max, required) in [
        (&input.title, 200, true),
        (&input.description, 5000, true),
        (&input.steps, 2000, false),
        (&input.expected, 2000, false),
    ] {
        if (required && value.trim().is_empty())
            || value.chars().count() > max
            || value.contains('\0')
        {
            return Err("Invalid feedback field".into());
        }
    }
    let title = format!(
        "[{prefix}]: {}",
        input.title.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    let mut body = format!(
        "<!-- dcc-feedback:{} -->\n<!-- dcc-feedback-category:{} -->\n\n{}",
        input.request_id,
        input.category,
        input.description.trim()
    );
    if input.category == "bug" {
        for (heading, value) in [
            ("Steps to reproduce", &input.steps),
            ("Expected behavior", &input.expected),
        ] {
            if !value.trim().is_empty() {
                body.push_str(&format!("\n\n### {heading}\n{}", value.trim()));
            }
        }
    }
    if input.include_diagnostics {
        body.push_str(&format!(
            "\n\n### DCC environment\n- Version: {}\n- OS: {}\n- Architecture: {}",
            context.version, context.platform, context.architecture
        ));
    }
    Ok((title, body))
}

fn receipts(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|e| e.to_string())?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS dcc_feedback_receipts (
        request_id TEXT PRIMARY KEY, login TEXT NOT NULL, payload TEXT NOT NULL, issue_json TEXT
    );",
        )
        .map_err(|e| e.to_string())?;
    Ok(connection)
}

fn remember(connection: &Connection, id: &str, result: &FeedbackIssue) -> Result<(), String> {
    let json = serde_json::to_string(result).map_err(|e| e.to_string())?;
    connection
        .execute(
            "UPDATE dcc_feedback_receipts SET issue_json = ?1 WHERE request_id = ?2",
            params![json, id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn create(
    path: &Path,
    input: FeedbackInput,
    context: FeedbackContext,
) -> Result<FeedbackIssue, String> {
    let (title, body) = format_submission(&input, &context)?;
    let client = client(&input.login)?;
    let connection = receipts(path)?;
    deliver(
        &connection,
        &input,
        |page| raw_page(&client, &input.login, page),
        || {
            let response = client
                .post(format!("{API}/repos/{REPOSITORY}/issues"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(serde_json::json!({ "title": title, "body": body }).to_string())
                .send()
                .map_err(|_| UNCERTAIN.to_string())?;
            let status = response.status();
            if !status.is_success() {
                if status.is_client_error() && status.as_u16() != 408 {
                    return Err(format!("FEEDBACK_GITHUB_{}", status.as_u16()));
                }
                return Err(UNCERTAIN.into());
            }
            serde_json::from_str(&response.text().map_err(|_| UNCERTAIN.to_string())?)
                .map_err(|_| UNCERTAIN.into())
        },
    )
}

fn deliver(
    connection: &Connection,
    input: &FeedbackInput,
    fetch_page: impl Fn(u32) -> Result<Vec<Value>, String>,
    post: impl FnOnce() -> Result<Value, String>,
) -> Result<FeedbackIssue, String> {
    let payload = serde_json::to_string(input).map_err(|e| e.to_string())?;
    // Reserve before POST so a double click, timeout or restart cannot replay a write.
    let inserted = connection.execute(
        "INSERT OR IGNORE INTO dcc_feedback_receipts(request_id, login, payload) VALUES (?1, ?2, ?3)",
        params![input.request_id, input.login, payload],
    ).map_err(|e| e.to_string())?;
    if inserted == 0 {
        let (saved_payload, saved): (String, Option<String>) = connection
            .query_row(
                "SELECT payload, issue_json FROM dcc_feedback_receipts WHERE request_id = ?1",
                [&input.request_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        if saved_payload != payload {
            return Err("FEEDBACK_REQUEST_CHANGED".into());
        }
        if let Some(saved) = saved {
            return serde_json::from_str(&saved).map_err(|e| e.to_string());
        }
        // Direct repository listing avoids search-index lag after a lost POST response.
        let marker = format!("<!-- dcc-feedback:{} -->", input.request_id);
        for page in 1..=10 {
            let values = fetch_page(page).map_err(|_| UNCERTAIN)?;
            if let Some(value) = values.iter().find(|v| {
                v.get("pull_request").is_none()
                    && v["body"].as_str().is_some_and(|b| b.contains(&marker))
            }) {
                let result = issue(value)?;
                remember(&connection, &input.request_id, &result)?;
                return Ok(result);
            }
            if values.len() < PAGE_SIZE {
                break;
            }
        }
        return Err(UNCERTAIN.into());
    }
    let value = match post() {
        Ok(value) => value,
        Err(error) => {
            if error.starts_with("FEEDBACK_GITHUB_") {
                connection
                    .execute(
                        "DELETE FROM dcc_feedback_receipts WHERE request_id = ?1",
                        [&input.request_id],
                    )
                    .map_err(|e| e.to_string())?;
            }
            return Err(error);
        }
    };
    let result = issue(&value).map_err(|_| UNCERTAIN)?;
    remember(&connection, &input.request_id, &result).map_err(|_| UNCERTAIN)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    fn fixture() -> (FeedbackInput, FeedbackContext) {
        (
            FeedbackInput {
                request_id: uuid::Uuid::new_v4().to_string(),
                login: "alice".into(),
                category: "bug".into(),
                title: "  Sidebar fails  ".into(),
                description: "Details with `code` and $(text)".into(),
                steps: "Open sidebar".into(),
                expected: "It opens".into(),
                include_diagnostics: true,
            },
            FeedbackContext {
                login: "alice".into(),
                version: "1.2.3".into(),
                platform: "linux".into(),
                architecture: "x86_64".into(),
            },
        )
    }

    #[test]
    fn formats_bug_and_only_explicit_diagnostics() {
        let (mut input, context) = fixture();
        let (title, body) = format_submission(&input, &context).unwrap();
        assert_eq!(title, "[Bug]: Sidebar fails");
        assert!(body.contains("$(text)"));
        assert!(body.contains("Steps to reproduce\nOpen sidebar"));
        assert!(body.contains("Version: 1.2.3"));
        input.include_diagnostics = false;
        input.category = "improvement".into();
        let (_, body) = format_submission(&input, &context).unwrap();
        assert!(!body.contains("DCC environment"));
        assert!(!body.contains("Open sidebar"));
    }

    #[test]
    fn rejects_account_changes_empty_fields_and_unbounded_inputs() {
        let (mut input, mut context) = fixture();
        context.login = "bob".into();
        assert_eq!(
            format_submission(&input, &context).unwrap_err(),
            "FEEDBACK_ACCOUNT_CHANGED"
        );
        context.login = "alice".into();
        input.title = " ".into();
        assert!(format_submission(&input, &context).is_err());
        input.title = "x".repeat(201);
        assert!(format_submission(&input, &context).is_err());
    }

    #[test]
    fn preserves_closed_reason_and_constructs_fixed_repository_url() {
        let value = serde_json::json!({ "number": 42, "title": "[Feature]: More", "state": "closed",
            "state_reason": "not_planned", "created_at": "2026-09-11T12:00:00Z", "html_url": "https://evil.example" });
        let result = issue(&value).unwrap();
        assert_eq!(result.state_reason.as_deref(), Some("not_planned"));
        assert_eq!(result.category, "improvement");
        assert_eq!(
            result.url,
            "https://github.com/wharley/DevCommandCenter/issues/42"
        );
    }

    #[test]
    fn receipt_reservation_survives_reopening_and_prevents_duplicate_insert() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feedback.db");
        let connection = receipts(&path).unwrap();
        connection
            .execute(
                "INSERT INTO dcc_feedback_receipts VALUES ('id','alice','payload',NULL)",
                [],
            )
            .unwrap();
        drop(connection);
        let connection = receipts(&path).unwrap();
        assert_eq!(connection.execute("INSERT OR IGNORE INTO dcc_feedback_receipts VALUES ('id','alice','payload',NULL)", []).unwrap(), 0);
        let pending: Option<String> = connection
            .query_row(
                "SELECT issue_json FROM dcc_feedback_receipts WHERE request_id='id'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
            .flatten();
        assert!(pending.is_none());
    }

    fn github_issue(input: &FeedbackInput) -> Value {
        serde_json::json!({ "number": 42, "title": "[Bug]: Sidebar fails", "state": "open",
            "created_at": "2026-09-11T12:00:00Z", "body": format!("<!-- dcc-feedback:{} -->", input.request_id) })
    }

    #[test]
    fn lost_response_is_recovered_without_repeating_post() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feedback.db");
        let connection = receipts(&path).unwrap();
        let (input, _) = fixture();
        assert_eq!(
            deliver(
                &connection,
                &input,
                |_| panic!("fresh submission must not search"),
                || Err(UNCERTAIN.into())
            )
            .unwrap_err(),
            UNCERTAIN
        );
        drop(connection);
        let connection = receipts(&path).unwrap();
        let recovered = deliver(
            &connection,
            &input,
            |_| Ok(vec![github_issue(&input)]),
            || panic!("must not publish twice"),
        )
        .unwrap();
        assert_eq!(recovered.number, 42);
        let saved = deliver(
            &connection,
            &input,
            |_| panic!("receipt already exists"),
            || panic!("must not publish twice"),
        )
        .unwrap();
        assert_eq!(saved.number, 42);
    }

    #[test]
    fn uncertain_missing_issue_stays_pending_and_rejects_edited_payload() {
        let dir = tempfile::tempdir().unwrap();
        let connection = receipts(&dir.path().join("feedback.db")).unwrap();
        let (mut input, _) = fixture();
        let _ = deliver(
            &connection,
            &input,
            |_| Ok(vec![]),
            || Err(UNCERTAIN.into()),
        );
        assert_eq!(
            deliver(
                &connection,
                &input,
                |_| Ok(vec![]),
                || panic!("must not repeat uncertain POST")
            )
            .unwrap_err(),
            UNCERTAIN
        );
        input.login = "bob".into();
        assert_eq!(
            deliver(&connection, &input, |_| panic!(), || panic!()).unwrap_err(),
            "FEEDBACK_REQUEST_CHANGED"
        );
    }

    #[test]
    fn definite_rejection_can_be_corrected_and_resubmitted() {
        let dir = tempfile::tempdir().unwrap();
        let connection = receipts(&dir.path().join("feedback.db")).unwrap();
        let (mut input, _) = fixture();
        assert_eq!(
            deliver(
                &connection,
                &input,
                |_| panic!(),
                || Err("FEEDBACK_GITHUB_422".into())
            )
            .unwrap_err(),
            "FEEDBACK_GITHUB_422"
        );
        input.description = "Corrected report".into();
        assert_eq!(
            deliver(
                &connection,
                &input,
                |_| panic!(),
                || Ok(github_issue(&input))
            )
            .unwrap()
            .number,
            42
        );
    }
}
