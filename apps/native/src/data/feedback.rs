//! Feedback (`feedback-api.ts`): the draft kept on this device across
//! openings (a request id per report, so a retry never publishes twice),
//! the last page of "Meus feedbacks" for when GitHub cannot be reached, the
//! errors as the dialog names them, and the calls into
//! `dcc_infra::feedback`, behind a trait the tests replace.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dcc_infra::feedback::{FeedbackContext, FeedbackInput, FeedbackIssue, FeedbackPage};
use serde::{Deserialize, Serialize};

/// `FEEDBACK_REPOSITORY`
pub const REPOSITORY: &str = "wharley/DevCommandCenter";

/// The DCC version the diagnostics name: the desktop's, the same release.
pub fn version() -> String {
    const CONFIG: &str = include_str!("../../../../src-tauri/tauri.conf.json");
    serde_json::from_str::<serde_json::Value>(CONFIG)
        .ok()
        .and_then(|config| config["version"].as_str().map(str::to_string))
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    #[default]
    Bug,
    Improvement,
    Other,
}

impl Category {
    pub const ALL: [Category; 3] = [Category::Bug, Category::Improvement, Category::Other];

    pub fn key(self) -> &'static str {
        match self {
            Category::Bug => "bug",
            Category::Improvement => "improvement",
            Category::Other => "other",
        }
    }

    /// The issue title's prefix (`[Bug]`, `[Feature]`, `[Feedback]`).
    pub fn prefix(self) -> &'static str {
        match self {
            Category::Bug => "Bug",
            Category::Improvement => "Feature",
            Category::Other => "Feedback",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Category::Bug => "bug",
            Category::Improvement => "lightbulb",
            Category::Other => "message-square-more",
        }
    }
}

/// `FeedbackDraft`: the fields, and the request already sent (`pending`)
/// while its outcome is unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub request_id: String,
    pub category: Category,
    pub title: String,
    pub description: String,
    pub steps: String,
    pub expected: String,
    pub include_diagnostics: bool,
    #[serde(default)]
    pub pending: Option<PendingRequest>,
}

/// A `FeedbackInput` as the draft keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRequest {
    pub request_id: String,
    pub login: String,
    pub category: Category,
    pub title: String,
    pub description: String,
    pub steps: String,
    pub expected: String,
    pub include_diagnostics: bool,
}

impl PendingRequest {
    pub fn input(&self) -> FeedbackInput {
        FeedbackInput {
            request_id: self.request_id.clone(),
            login: self.login.clone(),
            category: self.category.key().to_string(),
            title: self.title.clone(),
            description: self.description.clone(),
            steps: self.steps.clone(),
            expected: self.expected.clone(),
            include_diagnostics: self.include_diagnostics,
        }
    }
}

impl Draft {
    /// `emptyFeedbackDraft`
    pub fn empty() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            category: Category::Bug,
            title: String::new(),
            description: String::new(),
            steps: String::new(),
            expected: String::new(),
            include_diagnostics: true,
            pending: None,
        }
    }

    /// What goes out for `login`: the pending request when there is one
    /// (the same request id, so it is looked up, not published again).
    pub fn request(&self, login: &str) -> PendingRequest {
        self.pending.clone().unwrap_or_else(|| PendingRequest {
            request_id: self.request_id.clone(),
            login: login.to_string(),
            category: self.category,
            title: self.title.clone(),
            description: self.description.clone(),
            steps: self.steps.clone(),
            expected: self.expected.clone(),
            include_diagnostics: self.include_diagnostics,
        })
    }

    /// The issue's title: `[Bug]: words joined by one space`.
    pub fn report_title(&self) -> String {
        format!(
            "[{}]: {}",
            self.category.prefix(),
            self.title.split_whitespace().collect::<Vec<_>>().join(" ")
        )
    }
}

/// `feedbackStatus`
pub fn status(issue: &FeedbackIssue) -> &'static str {
    if issue.state == "open" {
        return "open";
    }
    match issue.state_reason.as_deref() {
        Some("completed") => "completed",
        Some("not_planned") => "notPlanned",
        _ => "closed",
    }
}

/// `feedbackError`: the `feedback.errors.*` key for a failure.
pub fn error_key(error: &str) -> &'static str {
    if error.contains("FEEDBACK_UNCERTAIN") {
        "uncertain"
    } else if error.contains("FEEDBACK_ACCOUNT_CHANGED") {
        "accountChanged"
    } else if error.contains("FEEDBACK_AUTH_REQUIRED") || error.contains("FEEDBACK_GITHUB_401") {
        "authRequired"
    } else if error.contains("FEEDBACK_GITHUB_403") || error.contains("FEEDBACK_GITHUB_429") {
        "forbidden"
    } else if error.contains("FEEDBACK_GITHUB_422") {
        "rejected"
    } else {
        "failed"
    }
}

/// The issue's page, or the search of this account's issues.
pub fn issue_url(number: Option<u64>, login: Option<&str>) -> String {
    match number {
        Some(number) => format!("https://github.com/{REPOSITORY}/issues/{number}"),
        None => {
            let query = format!("is:issue author:{}", login.unwrap_or("@me"));
            let encoded: String = query
                .bytes()
                .map(|byte| match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (byte as char).to_string()
                    }
                    other => format!("%{other:02X}"),
                })
                .collect();
            format!("https://github.com/{REPOSITORY}/issues?q={encoded}")
        }
    }
}

/// What is kept on this device: the draft (`dcc.feedback.draft.v1`) and the
/// history pages by login and page (`dcc.feedback.history.v1.<login>.<page>`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Stored {
    draft: Option<Draft>,
    #[serde(default)]
    history: HashMap<String, StoredPage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredPage {
    issues: Vec<FeedbackIssue>,
    has_next: bool,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_file() -> Self {
        Self::new(crate::prefs::native_file("feedback.json"))
    }

    fn read(&self) -> Stored {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, stored: &Stored) -> bool {
        let Ok(text) = serde_json::to_string(stored) else {
            return false;
        };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.path, text).is_ok()
    }

    /// `readFeedbackDraft`: a damaged or missing draft starts a new one.
    pub fn draft(&self) -> Draft {
        self.read().draft.unwrap_or_else(Draft::empty)
    }

    /// `saveFeedbackDraft`: false when it could not be kept.
    pub fn save_draft(&self, draft: &Draft) -> bool {
        let mut stored = self.read();
        stored.draft = Some(draft.clone());
        self.write(&stored)
    }

    pub fn history(&self, login: &str, page: u32) -> Option<FeedbackPage> {
        self.read()
            .history
            .get(&history_key(login, page))
            .map(|stored| FeedbackPage {
                issues: stored.issues.clone(),
                has_next: stored.has_next,
            })
    }

    pub fn save_history(&self, login: &str, page: u32, data: &FeedbackPage) {
        let mut stored = self.read();
        stored.history.insert(
            history_key(login, page),
            StoredPage {
                issues: data.issues.clone(),
                has_next: data.has_next,
            },
        );
        self.write(&stored);
    }
}

fn history_key(login: &str, page: u32) -> String {
    format!("{}.{page}", login.to_lowercase())
}

/// Who feedback goes out as, its pages and the publication.
pub trait Backend: Send + Sync {
    fn context(&self) -> Result<FeedbackContext, String>;
    fn list(&self, login: &str, page: u32) -> Result<FeedbackPage, String>;
    fn create(&self, input: FeedbackInput) -> Result<FeedbackIssue, String>;
}

/// The real one: GitHub through `gh`'s account, receipts in the database.
pub struct GitHub {
    pub db_path: PathBuf,
}

impl Backend for GitHub {
    fn context(&self) -> Result<FeedbackContext, String> {
        dcc_infra::feedback::context(&self.db_path, version())
    }

    /// `dcc_feedback_list`: refused when the account changed meanwhile.
    fn list(&self, login: &str, page: u32) -> Result<FeedbackPage, String> {
        let current = self.context()?;
        if current.login != login {
            return Err("FEEDBACK_ACCOUNT_CHANGED".into());
        }
        dcc_infra::feedback::list(login, page)
    }

    fn create(&self, input: FeedbackInput) -> Result<FeedbackIssue, String> {
        let context = self.context()?;
        dcc_infra::feedback::create(&self.db_path, input, context)
    }
}

pub type SharedBackend = Arc<dyn Backend>;

/// No account at all: what the shell's tests open, so they never reach
/// GitHub or `gh`.
#[cfg(test)]
pub struct Offline;

#[cfg(test)]
impl Backend for Offline {
    fn context(&self) -> Result<FeedbackContext, String> {
        Err("FEEDBACK_AUTH_REQUIRED".into())
    }

    fn list(&self, _: &str, _: u32) -> Result<FeedbackPage, String> {
        Err("FEEDBACK_AUTH_REQUIRED".into())
    }

    fn create(&self, _: FeedbackInput) -> Result<FeedbackIssue, String> {
        Err("FEEDBACK_AUTH_REQUIRED".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(state: &str, reason: Option<&str>) -> FeedbackIssue {
        FeedbackIssue {
            number: 1,
            title: "x".into(),
            url: String::new(),
            category: "bug".into(),
            state: state.into(),
            state_reason: reason.map(str::to_string),
            created_at: "2026-10-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn statuses_errors_and_urls_read_like_the_desktop() {
        assert_eq!(status(&issue("open", None)), "open");
        assert_eq!(status(&issue("closed", Some("completed"))), "completed");
        assert_eq!(status(&issue("closed", Some("not_planned"))), "notPlanned");
        assert_eq!(status(&issue("closed", None)), "closed");
        assert_eq!(error_key("FEEDBACK_GITHUB_429"), "forbidden");
        assert_eq!(error_key("FEEDBACK_AUTH_REQUIRED"), "authRequired");
        assert_eq!(error_key("boom"), "failed");
        assert_eq!(
            issue_url(Some(7), None),
            "https://github.com/wharley/DevCommandCenter/issues/7"
        );
        assert_eq!(
            issue_url(None, Some("ana")),
            "https://github.com/wharley/DevCommandCenter/issues?q=is%3Aissue%20author%3Aana"
        );
        assert!(!version().is_empty());
    }

    #[test]
    fn the_draft_and_history_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("feedback.json"));
        let mut draft = store.draft();
        assert_eq!(draft.category, Category::Bug);
        draft.title = "  Sidebar   fails ".into();
        assert!(store.save_draft(&draft));
        let again = Store::new(dir.path().join("feedback.json")).draft();
        assert_eq!(again, draft);
        assert_eq!(again.report_title(), "[Bug]: Sidebar fails");
        let request = again.request("ana");
        assert_eq!(request.request_id, draft.request_id);
        assert_eq!(request.input().category, "bug");
        let page = FeedbackPage {
            issues: vec![issue("open", None)],
            has_next: false,
        };
        store.save_history("Ana", 1, &page);
        assert_eq!(store.history("ana", 1).unwrap().issues.len(), 1);
        assert!(store.history("ana", 2).is_none());
        // A damaged file starts over instead of failing.
        std::fs::write(dir.path().join("feedback.json"), "{nope").unwrap();
        assert_eq!(store.draft().pending, None);
    }
}
