//! What the agents' check-in reads from the same search call as the
//! reviewed pull requests (`requested` and `issues` in `REVIEWED_QUERY`):
//! the open pull requests that ask for the person's review and the open
//! issues assigned to them, each with its project when one of the person's
//! checkouts is that repository. No `gh` is added for them.

use dcc_core::domain::repository::{github_slug, Repository};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// A pull request or an issue the person is asked about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubAsked {
    pub url: String,
    pub number: u32,
    /// `owner/name`
    pub repository: String,
    pub title: String,
    pub author: Option<String>,
    pub updated_at: String,
    /// The person's project for that repository, when DCC has one.
    pub project_id: Option<String>,
}

fn nodes<'a>(answer: &'a Value, search: &str) -> &'a [Value] {
    answer
        .get("data")
        .and_then(|data| data.get(search))
        .and_then(|search| search.get("nodes"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn asked(node: &Value) -> Option<PullRequestHubAsked> {
    let text = |key: &str| node.get(key).and_then(Value::as_str).map(str::to_string);
    Some(PullRequestHubAsked {
        url: text("url")?,
        number: node.get("number")?.as_u64()? as u32,
        repository: node
            .pointer("/repository/nameWithOwner")
            .and_then(Value::as_str)?
            .to_string(),
        title: text("title").unwrap_or_default(),
        author: node
            .pointer("/author/login")
            .and_then(Value::as_str)
            .map(str::to_string),
        updated_at: text("updatedAt").unwrap_or_default(),
        project_id: None,
    })
}

/// The pull requests that ask for the person's review.
pub fn requested_from_search(answer: &Value) -> Vec<PullRequestHubAsked> {
    nodes(answer, "requested")
        .iter()
        .filter_map(asked)
        .collect()
}

/// The issues assigned to the person.
pub fn issues_from_search(answer: &Value) -> Vec<PullRequestHubAsked> {
    nodes(answer, "issues").iter().filter_map(asked).collect()
}

/// Fills in each one's project from the person's checkouts.
pub fn link(items: &mut [PullRequestHubAsked], repositories: &[Repository]) {
    for item in items.iter_mut() {
        let wanted = item.repository.to_lowercase();
        item.project_id = repositories
            .iter()
            .filter(|repository| {
                repository
                    .remote_url
                    .as_deref()
                    .and_then(github_slug)
                    .as_deref()
                    == Some(wanted.as_str())
            })
            .min_by(|left, right| left.created_at.cmp(&right.created_at))
            .map(|repository| repository.project_id.0.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::repository::RepositoryId;

    #[test]
    fn review_requests_and_issues_come_from_the_same_answer_with_their_project() {
        let answer = serde_json::json!({"data": {
            "requested": {"nodes": [
                {"url": "https://github.com/acme/app/pull/12", "number": 12, "title": "Checkout v2", "repository": {"nameWithOwner": "Acme/App"}, "author": {"login": "ana"}, "updatedAt": "2026-11-08T10:00:00Z"},
                {"number": 3}
            ]},
            "issues": {"nodes": [
                {"url": "https://github.com/other/x/issues/4", "number": 4, "title": "Bug no frete", "repository": {"nameWithOwner": "other/x"}, "updatedAt": "2026-11-08T09:00:00Z"}
            ]}
        }});
        let mut requested = requested_from_search(&answer);
        let mut issues = issues_from_search(&answer);
        assert_eq!(requested.len(), 1, "a node without a url is skipped");
        assert_eq!(requested[0].author.as_deref(), Some("ana"));
        assert_eq!(issues[0].title, "Bug no frete");
        let repository = Repository {
            id: RepositoryId("/loja".into()),
            project_id: ProjectId("p-loja".into()),
            name: "app".into(),
            display_name: None,
            icon: None,
            color: None,
            logo: None,
            pinned_at: None,
            root_path: "/loja".into(),
            base_branch: "main".into(),
            remote: None,
            remote_url: Some("git@github.com:acme/app.git".into()),
            forge_provider: None,
            forge_login: None,
            created_at: "1".into(),
            updated_at: "1".into(),
        };
        link(&mut requested, std::slice::from_ref(&repository));
        link(&mut issues, std::slice::from_ref(&repository));
        assert_eq!(requested[0].project_id.as_deref(), Some("p-loja"));
        assert_eq!(issues[0].project_id, None);
        assert!(requested_from_search(&serde_json::json!({})).is_empty());
    }
}
