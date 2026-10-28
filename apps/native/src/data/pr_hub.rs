//! The "Pull Requests" hub (`pull-requests-hub.tsx`): the open pull requests
//! across the projects (with the filters and the search), one's detail, a
//! comment, and how its times read. Through the desktop's
//! `/api/v1/pull-request-hub/*`. Blocking: call it off the UI thread.

use serde::Deserialize;
use serde_json::{Value, json};

use super::desktop_api::{ApiError, DesktopApi};

/// `PullRequestHubActor`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Actor {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// `PullRequestHubItem`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    pub id: String,
    /// `github` | `gitlab`
    pub provider: String,
    pub project_id: String,
    pub repository_name: String,
    pub repository_root: String,
    pub forge_login: Option<String>,
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    pub author: Option<Actor>,
    pub head_branch: String,
    pub base_branch: String,
    pub is_draft: bool,
    pub review_requested_for_viewer: bool,
    pub created_by_viewer: bool,
    pub reviewers: Vec<Actor>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub comment_count: u64,
    /// `success` | `failure` | `pending` | …
    pub checks_state: String,
    pub updated_at: Option<String>,
    pub linked_workspace_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Warning {
    pub repository_name: String,
    pub message: String,
}

/// `PullRequestHubListOutput`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct List {
    pub items: Vec<Item>,
    pub warnings: Vec<Warning>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Check {
    pub name: String,
    pub state: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Comment {
    pub id: String,
    pub body: String,
    pub author: Option<Actor>,
    pub created_at: Option<String>,
}

/// `PullRequestHubMergeCapabilities`
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeCapabilities {
    /// `merge` | `squash` | `rebase`
    pub allowed_methods: Vec<String>,
    pub viewer_can_merge: bool,
    pub mergeable: Option<String>,
    pub head_sha: Option<String>,
}

/// `MERGE_METHODS`, in the menu's order.
pub const MERGE_METHODS: [&str; 3] = ["merge", "squash", "rebase"];

/// `PullRequestHubDetailOutput`, what the summary reads.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Detail {
    pub body: Option<String>,
    pub comments: Vec<Comment>,
    pub checks: Vec<Check>,
    /// GitHub only.
    pub merge_capabilities: Option<MergeCapabilities>,
    /// The rest (files, inline comments, capabilities) for the code tab and
    /// the merge.
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    /// Waiting on the person's review.
    Reviewing,
    /// Opened by the person.
    Mine,
}

impl Filter {
    pub const ALL: [Filter; 3] = [Filter::All, Filter::Reviewing, Filter::Mine];

    pub fn key(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Reviewing => "reviewing",
            Filter::Mine => "mine",
        }
    }
}

/// `filteredItems`: the filter, then the search over title, repository,
/// author and branch.
pub fn filtered<'a>(items: &'a [Item], filter: Filter, search: &str) -> Vec<&'a Item> {
    let needle = search.trim().to_lowercase();
    items
        .iter()
        .filter(|item| match filter {
            Filter::All => true,
            Filter::Reviewing => item.review_requested_for_viewer,
            Filter::Mine => item.created_by_viewer,
        })
        .filter(|item| {
            needle.is_empty()
                || format!(
                    "{} {} {} {}",
                    item.title,
                    item.repository_name,
                    item.author
                        .as_ref()
                        .map(|actor| actor.login.as_str())
                        .unwrap_or_default(),
                    item.head_branch
                )
                .to_lowercase()
                .contains(&needle)
        })
        .collect()
}

/// `relativeDate`: minutes (at least one), hours, then days, as
/// `Intl.RelativeTimeFormat(…, { numeric: "auto" })` says them.
pub fn relative_date(value: Option<&str>, now_ms: i64, english: bool) -> String {
    let Some(at) = value.and_then(super::time::parse_ms) else {
        return String::new();
    };
    let minutes = ((now_ms - at) / 60_000).max(1);
    let (count, unit) = if minutes < 60 {
        (minutes, "minute")
    } else if minutes / 60 < 24 {
        (minutes / 60, "hour")
    } else {
        (minutes / 60 / 24, "day")
    };
    let plural = count != 1;
    match (english, unit) {
        (true, "day") if count == 1 => "yesterday".into(),
        (false, "day") if count == 1 => "ontem".into(),
        (true, unit) => format!("{count} {unit}{} ago", if plural { "s" } else { "" }),
        (false, "minute") => format!("há {count} minuto{}", if plural { "s" } else { "" }),
        (false, "hour") => format!("há {count} hora{}", if plural { "s" } else { "" }),
        (false, _) => format!("há {count} dia{}", if plural { "s" } else { "" }),
    }
}

/// `mergeUnavailableReason`: why "Mesclar" is off, as an i18n key; `None`
/// when it can be used. `detail` is `None` while it is read.
pub fn merge_blocked(item: &Item, detail: Option<&Result<Detail, String>>) -> Option<&'static str> {
    let detail = match detail {
        None => return Some("pullRequests.merge.loadingCapabilities"),
        Some(Err(_)) => return Some("pullRequests.merge.capabilitiesUnavailable"),
        Some(Ok(detail)) => detail,
    };
    let Some(merge) = &detail.merge_capabilities else {
        return Some("pullRequests.merge.unsupportedProvider");
    };
    if !merge.viewer_can_merge {
        return Some("pullRequests.merge.noPermission");
    }
    if item.is_draft {
        return Some("pullRequests.merge.draftBlocked");
    }
    if merge.mergeable.as_deref() == Some("CONFLICTING") {
        return Some("pullRequests.merge.conflictsBlocked");
    }
    if merge.head_sha.as_deref().is_none_or(str::is_empty) {
        return Some("pullRequests.merge.commitUnavailable");
    }
    if merge.allowed_methods.is_empty() {
        return Some("pullRequests.merge.noMethods");
    }
    None
}

/// What the forge did with a merge, as the toast's key.
pub fn merge_toast(status: &str) -> &'static str {
    match status {
        "merged" => "pullRequests.merge.success",
        "queued" => "pullRequests.merge.queued",
        _ => "pullRequests.merge.submitted",
    }
}

fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
}

impl DesktopApi {
    /// `pullRequestHubList`
    pub fn pr_hub_list(&self) -> Result<List, ApiError> {
        decode(self.post("/api/v1/pull-request-hub/list", &json!({}))?)
    }

    /// `pullRequestHubDetail`; `include_code` reads the patches too.
    pub fn pr_hub_detail(&self, item: &Item, include_code: bool) -> Result<Detail, ApiError> {
        decode(self.post(
            "/api/v1/pull-request-hub/detail",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "forgeLogin": item.forge_login,
                "includeCode": include_code,
            }),
        )?)
    }

    /// `pullRequestHubMerge` on the reviewed commit; the forge's status.
    pub fn pr_hub_merge(
        &self,
        item: &Item,
        method: &str,
        head_sha: &str,
    ) -> Result<String, ApiError> {
        let value = self.post(
            "/api/v1/pull-request-hub/merge",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "method": method,
                "expectedHeadSha": head_sha,
                "forgeLogin": item.forge_login,
            }),
        )?;
        Ok(value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("submitted")
            .to_string())
    }

    /// `pullRequestHubComment`
    pub fn pr_hub_comment(&self, item: &Item, body: &str) -> Result<(), ApiError> {
        self.post(
            "/api/v1/pull-request-hub/comment",
            &json!({
                "repositoryRoot": item.repository_root,
                "number": item.number,
                "body": body,
                "forgeLogin": item.forge_login,
            }),
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, title: &str, reviewing: bool, mine: bool) -> Item {
        Item {
            id: id.into(),
            title: title.into(),
            repository_name: "acme/app".into(),
            head_branch: "feat/login".into(),
            review_requested_for_viewer: reviewing,
            created_by_viewer: mine,
            author: Some(Actor {
                login: "ana".into(),
                ..Actor::default()
            }),
            ..Item::default()
        }
    }

    #[test]
    fn the_filters_and_the_search_narrow_the_list() {
        let items = [
            item("1", "Fix login", true, false),
            item("2", "Docs", false, true),
        ];
        assert_eq!(filtered(&items, Filter::All, "").len(), 2);
        assert_eq!(filtered(&items, Filter::Reviewing, "")[0].id, "1");
        assert_eq!(filtered(&items, Filter::Mine, "")[0].id, "2");
        assert_eq!(filtered(&items, Filter::All, "ANA").len(), 2);
        assert_eq!(
            filtered(&items, Filter::All, "login").len(),
            2,
            "the branch counts"
        );
        assert_eq!(filtered(&items, Filter::All, "docs")[0].id, "2");
    }

    #[test]
    fn merge_reads_its_reason_like_the_desktop() {
        let item = item("1", "Fix", false, false);
        assert_eq!(
            merge_blocked(&item, None),
            Some("pullRequests.merge.loadingCapabilities")
        );
        let detail = |merge: Option<MergeCapabilities>| {
            Ok(Detail {
                merge_capabilities: merge,
                ..Detail::default()
            })
        };
        assert_eq!(
            merge_blocked(&item, Some(&detail(None))),
            Some("pullRequests.merge.unsupportedProvider")
        );
        let ready = MergeCapabilities {
            allowed_methods: vec!["squash".into()],
            viewer_can_merge: true,
            mergeable: Some("MERGEABLE".into()),
            head_sha: Some("abc".into()),
        };
        assert_eq!(
            merge_blocked(&item, Some(&detail(Some(ready.clone())))),
            None
        );
        let draft = Item {
            is_draft: true,
            ..item.clone()
        };
        assert_eq!(
            merge_blocked(&draft, Some(&detail(Some(ready.clone())))),
            Some("pullRequests.merge.draftBlocked")
        );
        let conflicting = MergeCapabilities {
            mergeable: Some("CONFLICTING".into()),
            ..ready.clone()
        };
        assert_eq!(
            merge_blocked(&item, Some(&detail(Some(conflicting)))),
            Some("pullRequests.merge.conflictsBlocked")
        );
        let no_sha = MergeCapabilities {
            head_sha: None,
            ..ready
        };
        assert_eq!(
            merge_blocked(&item, Some(&detail(Some(no_sha)))),
            Some("pullRequests.merge.commitUnavailable")
        );
        assert_eq!(merge_toast("queued"), "pullRequests.merge.queued");
    }

    #[test]
    fn times_read_like_intl_relative() {
        let now = super::super::time::parse_ms("2026-10-27T12:00:00Z").unwrap();
        let at = |value: &str, english| relative_date(Some(value), now, english);
        assert_eq!(at("2026-10-27T11:59:50Z", false), "há 1 minuto");
        assert_eq!(at("2026-10-27T11:30:00Z", false), "há 30 minutos");
        assert_eq!(at("2026-10-27T09:00:00Z", true), "3 hours ago");
        assert_eq!(at("2026-10-26T10:00:00Z", false), "ontem");
        assert_eq!(at("2026-10-24T10:00:00Z", true), "3 days ago");
        assert_eq!(relative_date(None, now, false), "");
    }
}
