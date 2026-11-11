use super::*;
use serde_json::json;

fn args(value: serde_json::Value) -> Result<GithubRead, String> {
    serde_json::from_value::<GithubReadArgs>(value)
        .map_err(|error| error.to_string())
        .and_then(|args| args.checked())
}

fn slug() -> RepoSlug {
    RepoSlug::parse("acme/app").unwrap()
}

#[test]
fn a_repository_is_owner_and_name_only() {
    assert_eq!(
        RepoSlug::parse("acme/app.web").unwrap().full(),
        "acme/app.web"
    );
    for bad in [
        "acme",
        "acme/app/x",
        "acme/",
        "/app",
        "-acme/app",
        "acme/..",
        "acme/app;rm",
        "acme/app name",
        "https://github.com/acme/app",
    ] {
        assert!(RepoSlug::parse(bad).is_none(), "{bad}");
    }
}

#[test]
fn bad_inputs_are_refused() {
    for bad in [
        json!({"operation": "pull_requests", "since": "monday"}),
        json!({"operation": "pull_requests", "since": "2026-11-09T11:00:00"}),
        json!({"operation": "pull_requests", "since": "2026-11-10", "until": "2026-11-09"}),
        json!({"operation": "pull_requests", "limit": 0}),
        json!({"operation": "pull_requests", "limit": 101}),
        json!({"operation": "pull_requests", "author": "a b"}),
        json!({"operation": "pull_requests", "author": "--repo=x/y"}),
        json!({"operation": "pull_requests", "base": "-x"}),
        json!({"operation": "pull_requests", "base": "a..b"}),
        json!({"operation": "pull_requests", "search": "fix repo:other/secret"}),
        json!({"operation": "pull_requests", "search": "a\nb"}),
        json!({"operation": "pull_requests", "state": "deleted"}),
        json!({"operation": "pull_requests", "number": 3}),
        json!({"operation": "pull_requests", "method": "POST"}),
        json!({"operation": "pull_request"}),
        json!({"operation": "pull_request", "number": 0}),
        json!({"operation": "pull_request", "number": 3, "since": "2026-11-09"}),
        json!({"operation": "deployments", "sha": "xyz1234"}),
        json!({"operation": "deployments", "sha": "abc"}),
        json!({"operation": "deployments", "sha": "abcdef1", "number": 3}),
        json!({"operation": "deployments", "environment": "prod;x"}),
        json!({"operation": "merge", "number": 3}),
    ] {
        assert!(args(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn merged_since_builds_one_search_in_fixed_slots() {
    let read = args(json!({
        "operation": "pull_requests",
        "state": "merged",
        "since": "2026-11-09T11:00:00-03:00",
        "base": "main",
        "author": "octo-cat",
        "limit": 50
    }))
    .unwrap();
    let GithubRead::PullRequests(query) = read else {
        panic!("a list");
    };
    let argv = list_argv(&slug(), &query);
    assert_eq!(
        &argv[..8],
        ["pr", "list", "--repo", "acme/app", "--state", "merged", "--limit", "50"]
    );
    assert!(argv.contains(&"--author=octo-cat".to_string()));
    assert!(argv.contains(&"--base=main".to_string()));
    assert_eq!(
        argv.last().unwrap(),
        "--search=merged:>=2026-11-09T11:00:00-03:00"
    );
    assert!(is_read_only_argv(&argv));
}

#[test]
fn approved_by_me_searches_my_reviews() {
    let GithubRead::PullRequests(query) = args(json!({
        "operation": "pull_requests",
        "state": "all",
        "since": "2026-11-01",
        "until": "2026-11-09",
        "approvedByMe": true,
        "search": "checkout"
    }))
    .unwrap() else {
        panic!("a list");
    };
    let argv = list_argv(&slug(), &query);
    assert_eq!(
        argv.last().unwrap(),
        "--search=updated:2026-11-01..2026-11-09 reviewed-by:@me checkout"
    );
    assert!(argv.iter().any(|arg| arg.ends_with(",latestReviews")));
}

#[test]
fn every_built_call_is_a_read() {
    let repo = slug();
    let built = [
        view_argv(&repo, 12),
        deployments_argv("github.com", &repo, Some("abcdef1"), Some("Production"), 10),
        deployment_status_argv("github.com", &repo, 99),
        commit_status_argv("github.com", &repo, "abcdef1"),
        compare_argv("github.com", &repo, "abcdef1", "1234567"),
        viewer_argv("github.com"),
    ];
    for argv in &built {
        assert!(is_read_only_argv(argv), "{argv:?}");
        for word in [
            "merge", "close", "edit", "comment", "review", "create", "POST", "PATCH", "DELETE",
        ] {
            assert!(!argv.iter().any(|arg| arg == word), "{word} in {argv:?}");
        }
    }
    assert_eq!(
        built[1].last().unwrap(),
        "repos/acme/app/deployments?per_page=10&sha=abcdef1&environment=Production"
    );
    assert_eq!(
        deployments_argv("github.com", &repo, None, Some("Preview env"), 5)
            .last()
            .unwrap(),
        "repos/acme/app/deployments?per_page=5&environment=Preview%20env"
    );
    let refused = |parts: &[&str]| {
        !is_read_only_argv(&parts.iter().map(|p| p.to_string()).collect::<Vec<_>>())
    };
    assert!(refused(&["pr", "merge", "1"]));
    assert!(refused(&["pr", "list", "-X", "POST"]));
    assert!(refused(&["api", "--method", "POST", "repos/a/b/pulls"]));
    assert!(refused(&[
        "api",
        "--method",
        "GET",
        "-f",
        "x=1",
        "repos/a/b"
    ]));
    assert!(refused(&[
        "api",
        "--method",
        "GET",
        "--method",
        "DELETE",
        "repos/a/b"
    ]));
    assert!(refused(&["api", "--method", "GET", "graphql"]));
    assert!(refused(&["repo", "delete", "a/b"]));
}

#[test]
fn times_keep_their_offset() {
    assert_eq!(check_time("2026-11-09").unwrap(), "2026-11-09");
    assert_eq!(
        check_time("2026-11-09T14:00:00Z").unwrap(),
        "2026-11-09T14:00:00+00:00"
    );
    assert!(check_time("09/11/2026").is_none());
    assert_eq!(check_login("@me").unwrap(), "@me");
    assert_eq!(check_login("app/vercel").unwrap(), "app/vercel");
    assert_eq!(check_sha("ABCDEF1").unwrap(), "abcdef1");
}
