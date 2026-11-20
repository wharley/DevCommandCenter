//! People's pictures on GitHub. `gh pr view --json` names the author, the
//! reviewers asked and the reviews' authors by `login` only, so the views
//! drew initials where the desktop shows faces. Every GitHub host (github.com
//! and Enterprise) serves a user's picture at `https://<host>/<login>.png`,
//! so a missing `avatarUrl` is filled from the login, with no extra call.

use super::forge_commands::{PullRequestHubActor, PullRequestHubDetailOutput, WorkspaceReviewer};

/// The picture's address for `login` on `host`; none for a team, a bot or
/// an app (`app/x`, `x[bot]`), which have no `.png` of their own.
pub(crate) fn github_avatar_url(host: &str, login: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('/');
    let login = login.trim();
    let valid = !login.is_empty() && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (valid && !host.is_empty()).then(|| format!("https://{host}/{login}.png?size=96"))
}

pub(crate) fn fill_actor(actor: &mut PullRequestHubActor, host: &str) {
    if actor.avatar_url.as_deref().is_none_or(str::is_empty) {
        actor.avatar_url = github_avatar_url(host, &actor.login);
    }
}

pub(crate) fn fill_reviewer(reviewer: &mut WorkspaceReviewer, host: &str) {
    if reviewer.is_team
        || reviewer
            .avatar_url
            .as_deref()
            .is_some_and(|url| !url.is_empty())
    {
        return;
    }
    reviewer.avatar_url = reviewer
        .login
        .as_deref()
        .and_then(|login| github_avatar_url(host, login));
}

/// Every person a pull request's detail names: comments, line comments and
/// reviews.
pub(crate) fn fill_detail(detail: &mut PullRequestHubDetailOutput, host: &str) {
    let actors = detail
        .comments
        .iter_mut()
        .filter_map(|comment| comment.author.as_mut())
        .chain(
            detail
                .inline_comments
                .iter_mut()
                .filter_map(|comment| comment.author.as_mut()),
        )
        .chain(
            detail
                .reviews
                .iter_mut()
                .filter_map(|review| review.author.as_mut()),
        );
    for actor in actors {
        fill_actor(actor, host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(login: &str, avatar_url: Option<&str>) -> PullRequestHubActor {
        PullRequestHubActor {
            login: login.into(),
            name: None,
            avatar_url: avatar_url.map(Into::into),
            html_url: None,
        }
    }

    #[test]
    fn a_login_without_a_picture_gets_the_hosts_png() {
        let mut person = actor("keppelen", None);
        fill_actor(&mut person, "github.com");
        assert_eq!(
            person.avatar_url.as_deref(),
            Some("https://github.com/keppelen.png?size=96")
        );
        let mut enterprise = actor("ana-lima", Some(""));
        fill_actor(&mut enterprise, "git.example.com/");
        assert_eq!(
            enterprise.avatar_url.as_deref(),
            Some("https://git.example.com/ana-lima.png?size=96")
        );
    }

    #[test]
    fn a_picture_given_teams_and_bots_are_left_alone() {
        let mut person = actor("bia", Some("https://avatars.example/b.png"));
        fill_actor(&mut person, "github.com");
        assert_eq!(
            person.avatar_url.as_deref(),
            Some("https://avatars.example/b.png")
        );
        assert_eq!(github_avatar_url("github.com", "app/dependabot"), None);
        assert_eq!(github_avatar_url("github.com", "copilot[bot]"), None);
        let mut team = WorkspaceReviewer {
            login: Some("core".into()),
            name: Some("core".into()),
            avatar_url: None,
            html_url: None,
            state: "pending".into(),
            is_team: true,
        };
        fill_reviewer(&mut team, "github.com");
        assert_eq!(team.avatar_url, None);
        team.is_team = false;
        fill_reviewer(&mut team, "github.com");
        assert_eq!(
            team.avatar_url.as_deref(),
            Some("https://github.com/core.png?size=96")
        );
    }
}
