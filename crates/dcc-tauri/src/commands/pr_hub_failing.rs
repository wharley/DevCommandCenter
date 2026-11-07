//! The person's own open pull requests whose checks fail, from the same
//! search call as the reviewed ones (`failing` in `REVIEWED_QUERY`), each with
//! the task it belongs to: the task made from that pull request or on its
//! branch, else the active task of that repository whose checkout is on it
//! (read with local git, only while something fails). The Reviewer's CI
//! trigger reads them; no `gh` is added for it.

use dcc_core::domain::repository::{github_slug, Repository};
use dcc_core::domain::workspace::{Workspace, WorkspaceSourceKind, WorkspaceState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestHubFailing {
    pub url: String,
    pub number: u32,
    /// `owner/name`
    pub repository: String,
    pub title: String,
    pub head_branch: String,
    /// The commit whose checks fail: a new push is a new failure.
    pub head_sha: String,
    /// The person's task for it, when DCC has one.
    pub workspace_id: Option<String>,
    pub project_id: Option<String>,
    /// That task's folder (its worktree, else the project's checkout).
    pub workspace_path: Option<String>,
}

/// The `failing` search of a reviewed-search answer, not linked yet.
pub fn from_search(answer: &Value) -> Vec<PullRequestHubFailing> {
    answer
        .get("data")
        .and_then(|data| data.get("failing"))
        .and_then(|search| search.get("nodes"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|node| {
            let text = |key: &str| node.get(key).and_then(Value::as_str).map(str::to_string);
            let head_sha = node
                .pointer("/commits/nodes/0/commit/oid")
                .and_then(Value::as_str)?
                .to_string();
            Some(PullRequestHubFailing {
                url: text("url")?,
                number: node.get("number")?.as_u64()? as u32,
                repository: node
                    .pointer("/repository/nameWithOwner")
                    .and_then(Value::as_str)?
                    .to_string(),
                title: text("title").unwrap_or_default(),
                head_branch: text("headRefName")?,
                head_sha,
                workspace_id: None,
                project_id: None,
                workspace_path: None,
            })
        })
        .collect()
}

fn path_of(workspace: &Workspace) -> String {
    workspace
        .worktree_path
        .clone()
        .unwrap_or_else(|| workspace.root_path.clone())
}

/// Fills in each pull request's task. `branch_of` reads a checkout's current
/// branch; it only runs for the repository's tasks the saved branch did not
/// settle.
pub fn link(
    failing: &mut [PullRequestHubFailing],
    repositories: &[Repository],
    workspaces: &[Workspace],
    branch_of: impl Fn(&str) -> Option<String>,
) {
    for pr in failing.iter_mut() {
        let wanted = pr.repository.to_lowercase();
        let Some(repository) = repositories.iter().find(|repository| {
            repository
                .remote_url
                .as_deref()
                .and_then(github_slug)
                .as_deref()
                == Some(wanted.as_str())
        }) else {
            continue;
        };
        let active: Vec<&Workspace> = workspaces
            .iter()
            .filter(|workspace| {
                workspace.root_path == repository.root_path
                    && !matches!(
                        workspace.state,
                        WorkspaceState::Archived | WorkspaceState::Completed
                    )
            })
            .collect();
        let saved = active.iter().find(|workspace| {
            workspace.source.as_ref().is_some_and(|source| {
                (source.kind == WorkspaceSourceKind::PullRequest
                    && source.change_request_number == Some(pr.number))
                    || source.head_branch == pr.head_branch
            })
        });
        let found = saved.copied().or_else(|| {
            active
                .iter()
                .find(|workspace| {
                    branch_of(&path_of(workspace)).as_deref() == Some(&pr.head_branch)
                })
                .copied()
        });
        if let Some(workspace) = found {
            pr.workspace_id = Some(workspace.id.0.clone());
            pr.project_id = Some(workspace.project_id.0.clone());
            pr.workspace_path = Some(path_of(workspace));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::project::ProjectId;
    use dcc_core::domain::repository::RepositoryId;
    use dcc_core::domain::workspace::{WorkspaceId, WorkspaceSource};
    use serde_json::json;

    fn repository(root: &str, remote: &str) -> Repository {
        Repository {
            id: RepositoryId(root.into()),
            project_id: ProjectId("p-loja".into()),
            name: "app".into(),
            display_name: None,
            icon: None,
            color: None,
            logo: None,
            pinned_at: None,
            root_path: root.into(),
            base_branch: "main".into(),
            remote: None,
            remote_url: Some(remote.into()),
            forge_provider: None,
            forge_login: None,
            created_at: "1".into(),
            updated_at: "1".into(),
        }
    }

    fn workspace(id: &str, worktree: Option<&str>, branch: Option<&str>) -> Workspace {
        Workspace {
            id: WorkspaceId(id.into()),
            project_id: ProjectId("p-loja".into()),
            name: None,
            root_path: "/loja".into(),
            base_branch: "main".into(),
            worktree_path: worktree.map(str::to_string),
            source: branch.map(|branch| WorkspaceSource {
                kind: WorkspaceSourceKind::Branch,
                url: String::new(),
                provider: "github".into(),
                remote_name: "origin".into(),
                head_branch: branch.into(),
                head_sha: String::new(),
                base_branch: "main".into(),
                change_request_number: None,
                title: None,
                author: None,
                source_repository: None,
                push_target: None,
            }),
            state: WorkspaceState::Ready,
            setup_report: None,
            pinned_at: None,
            snoozed_until: None,
            created_at: "1".into(),
            updated_at: "1".into(),
        }
    }

    #[test]
    fn a_failing_pull_request_finds_its_task_by_branch() {
        let answer = json!({"data": {"failing": {"nodes": [
            {"url": "https://github.com/acme/app/pull/7", "number": 7, "title": "Rastreio", "headRefName": "feat/rastreio", "repository": {"nameWithOwner": "Acme/App"}, "commits": {"nodes": [{"commit": {"oid": "abc"}}]}},
            {"url": "https://github.com/acme/app/pull/8", "number": 8, "title": "Local", "headRefName": "fix/local", "repository": {"nameWithOwner": "acme/app"}, "commits": {"nodes": [{"commit": {"oid": "def"}}]}},
            {"url": "https://github.com/other/x/pull/1", "number": 1, "title": "Fora", "headRefName": "x", "repository": {"nameWithOwner": "other/x"}, "commits": {"nodes": [{"commit": {"oid": "f00"}}]}},
            {"url": "https://github.com/acme/app/pull/9", "number": 9, "headRefName": "sem-commit", "repository": {"nameWithOwner": "acme/app"}, "commits": {"nodes": []}}
        ]}}});
        let mut failing = from_search(&answer);
        assert_eq!(failing.len(), 3, "a pull request with no commit is skipped");
        assert_eq!(failing[0].head_sha, "abc");

        let repositories = [repository("/loja", "git@github.com:acme/app.git")];
        let mut archived = workspace("w-old", Some("/wt/old"), Some("fix/local"));
        archived.state = WorkspaceState::Archived;
        let workspaces = [
            archived,
            workspace("w-rastreio", Some("/wt/rastreio"), Some("feat/rastreio")),
            workspace("w-local", None, None),
        ];
        let read = std::cell::RefCell::new(Vec::new());
        link(&mut failing, &repositories, &workspaces, |path| {
            read.borrow_mut().push(path.to_string());
            (path == "/loja").then(|| "fix/local".to_string())
        });
        assert_eq!(failing[0].workspace_id.as_deref(), Some("w-rastreio"));
        assert_eq!(failing[0].workspace_path.as_deref(), Some("/wt/rastreio"));
        assert_eq!(failing[1].workspace_id.as_deref(), Some("w-local"));
        assert_eq!(failing[1].workspace_path.as_deref(), Some("/loja"));
        assert_eq!(failing[1].project_id.as_deref(), Some("p-loja"));
        assert_eq!(
            failing[2].workspace_id, None,
            "not one of the person's projects"
        );
        assert!(
            !read.borrow().contains(&"/wt/old".to_string()),
            "an archived task's checkout is never read"
        );
    }
}
