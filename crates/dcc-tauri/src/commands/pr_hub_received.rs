//! Reviews the person received on their own open pull requests, from the
//! same search call as the reviewed ones (`mine` in `REVIEWED_QUERY`): the
//! latest review or comment by someone else — bots left out — and who it
//! came from, each with its task (`pr_hub_failing::find_task`). The
//! Reviewer's review trigger reads them; no `gh` is added for it.

use dcc_core::domain::repository::Repository;
use dcc_core::domain::workspace::Workspace;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use super::pr_hub_failing::find_task;

/// Reviewers named per pull request.
const NAMED: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubReceived {
    pub url: String,
    pub number: u32,
    /// `owner/name`
    pub repository: String,
    pub title: String,
    pub head_branch: String,
    /// The latest review or comment by someone else.
    pub last_activity: String,
    /// Who it came from, latest first.
    pub from: Vec<String>,
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    pub workspace_path: Option<String>,
}

/// The `mine` search of a reviewed-search answer: the pull requests someone
/// else reviewed or commented on, not linked yet.
pub fn from_search(answer: &Value) -> Vec<PullRequestHubReceived> {
    let data = answer.get("data");
    let Some(viewer) = data
        .and_then(|data| data.pointer("/viewer/login"))
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    data.and_then(|data| data.pointer("/mine/nodes"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|node| {
            let mut activity: Vec<(String, String)> = [
                ("/reviews/nodes", "submittedAt"),
                ("/comments/nodes", "createdAt"),
            ]
            .into_iter()
            .flat_map(|(list, at)| {
                node.pointer(list)
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(move |entry| {
                        let author = entry.get("author")?;
                        if author.get("__typename").and_then(Value::as_str) == Some("Bot") {
                            return None;
                        }
                        let login = author.get("login")?.as_str()?;
                        if login.eq_ignore_ascii_case(viewer) {
                            return None;
                        }
                        Some((entry.get(at)?.as_str()?.to_string(), login.to_string()))
                    })
            })
            .collect();
            // RFC 3339 times from one source sort as text.
            activity.sort_by(|left, right| right.0.cmp(&left.0));
            let (last_activity, _) = activity.first()?.clone();
            let mut from: Vec<String> = Vec::new();
            for (_, login) in activity {
                if !from.contains(&login) && from.len() < NAMED {
                    from.push(login);
                }
            }
            let text = |key: &str| node.get(key).and_then(Value::as_str).map(str::to_string);
            Some(PullRequestHubReceived {
                url: text("url")?,
                number: node.get("number")?.as_u64()? as u32,
                repository: node
                    .pointer("/repository/nameWithOwner")
                    .and_then(Value::as_str)?
                    .to_string(),
                title: text("title").unwrap_or_default(),
                head_branch: text("headRefName")?,
                last_activity,
                from,
                workspace_id: None,
                project_id: None,
                workspace_path: None,
            })
        })
        .collect()
}

/// Fills in each pull request's task.
pub fn link(
    received: &mut [PullRequestHubReceived],
    repositories: &[Repository],
    workspaces: &[Workspace],
    branch_of: impl Fn(&str) -> Option<String>,
) {
    for pr in received.iter_mut() {
        if let Some(task) = find_task(
            repositories,
            workspaces,
            &pr.repository,
            pr.number,
            &pr.head_branch,
            &branch_of,
        ) {
            pr.workspace_id = Some(task.workspace_id);
            pr.project_id = Some(task.project_id);
            pr.workspace_path = Some(task.workspace_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_people_other_than_the_person_count() {
        let answer = json!({"data": {"viewer": {"login": "wharley"}, "mine": {"nodes": [
            {"url": "u1", "number": 1, "title": "Rastreio", "headRefName": "feat/a", "repository": {"nameWithOwner": "acme/app"},
             "reviews": {"nodes": [
                {"author": {"__typename": "User", "login": "giovanni"}, "state": "COMMENTED", "submittedAt": "2026-11-07T10:00:00Z"},
                {"author": {"__typename": "User", "login": "Wharley"}, "state": "COMMENTED", "submittedAt": "2026-11-07T12:00:00Z"}
             ]},
             "comments": {"nodes": [
                {"author": {"__typename": "Bot", "login": "vercel"}, "createdAt": "2026-11-07T13:00:00Z"},
                {"author": {"__typename": "User", "login": "daniel"}, "createdAt": "2026-11-07T11:00:00Z"},
                {"author": {"__typename": "User", "login": "giovanni"}, "createdAt": "2026-11-07T09:00:00Z"}
             ]}},
            {"url": "u2", "number": 2, "title": "Só robô", "headRefName": "feat/b", "repository": {"nameWithOwner": "acme/app"},
             "reviews": {"nodes": []},
             "comments": {"nodes": [{"author": {"__typename": "Bot", "login": "vercel"}, "createdAt": "2026-11-07T13:00:00Z"}]}},
            {"url": "u3", "number": 3, "headRefName": "feat/c", "repository": {"nameWithOwner": "acme/app"},
             "reviews": {"nodes": [{"author": null, "submittedAt": "2026-11-07T13:00:00Z"}]}, "comments": {"nodes": []}}
        ]}}});
        let received = from_search(&answer);
        assert_eq!(received.len(), 1, "a bot or a ghost is no review");
        assert_eq!(received[0].last_activity, "2026-11-07T11:00:00Z");
        assert_eq!(received[0].from, ["daniel", "giovanni"]);
        assert!(from_search(&json!({"data": {}})).is_empty());
    }
}
