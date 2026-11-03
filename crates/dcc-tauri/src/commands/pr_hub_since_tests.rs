use serde_json::json;

use super::*;

/// The search as GitHub answers it: #1014 got a commit after the review,
/// #879 had nothing since, #500 was only reviewed by someone else.
fn answer() -> Value {
    json!({ "data": {
        "viewer": { "login": "wharley" },
        "search": { "nodes": [
            {
                "url": "https://github.com/acme/app/pull/1014", "number": 1014,
                "title": "perf(checkout): L1", "repository": { "nameWithOwner": "acme/app" },
                "author": { "login": "keppelen" },
                "latestReviews": { "nodes": [
                    { "author": { "login": "Wharley" }, "state": "COMMENTED",
                      "submittedAt": "2026-11-03T20:09:50Z" },
                    { "author": { "login": "ana" }, "state": "APPROVED",
                      "submittedAt": "2026-11-03T22:00:00Z" },
                ]},
                "commits": { "nodes": [
                    { "commit": { "committedDate": "2026-11-03T19:00:00Z" } },
                    { "commit": { "committedDate": "2026-11-03T21:43:14Z" } },
                ]},
                "comments": { "nodes": [
                    { "author": { "login": "keppelen" }, "createdAt": "2026-11-03T21:50:00Z" },
                    { "author": { "login": "keppelen" }, "createdAt": "2026-11-03T18:00:00Z" },
                    { "author": { "login": "wharley" }, "createdAt": "2026-11-03T22:10:00Z" },
                    { "author": { "login": "ana" }, "createdAt": "2026-11-03T22:20:00Z" },
                ]},
            },
            {
                "url": "https://github.com/acme/app/pull/879", "number": 879,
                "title": "Feat/automacao", "repository": { "nameWithOwner": "acme/app" },
                "author": { "login": "daniel" },
                "latestReviews": { "nodes": [
                    { "author": { "login": "wharley" }, "state": "COMMENTED",
                      "submittedAt": "2026-11-03T20:36:20Z" },
                ]},
                "commits": { "nodes": [
                    { "commit": { "committedDate": "2026-11-03T20:00:00Z" } },
                ]},
                "comments": { "nodes": [] },
            },
            {
                "url": "https://github.com/acme/app/pull/500", "number": 500,
                "title": "Other", "repository": { "nameWithOwner": "acme/app" },
                "author": { "login": "bob" },
                "latestReviews": { "nodes": [
                    { "author": { "login": "wharley" }, "state": "PENDING", "submittedAt": null },
                ]},
                "commits": { "nodes": [] }, "comments": { "nodes": [] },
            },
        ]},
    }})
}

#[test]
fn the_search_says_what_came_after_each_review() {
    let (viewer, items) = from_search(&answer());
    assert_eq!(viewer.as_deref(), Some("wharley"));
    let numbers: Vec<u32> = items.iter().map(|item| item.number).collect();
    assert_eq!(numbers, [1014, 879], "a pending review is not a review");
    assert_eq!(
        items[0].since,
        PullRequestHubSinceReview {
            reviewed_at: "2026-11-03T20:09:50Z".into(),
            commits: 1,
            replies: 1,
            last_activity: Some("2026-11-03T21:50:00Z".into()),
        }
    );
    assert_eq!(items[0].repository, "acme/app");
    assert_eq!(items[0].state, "COMMENTED", "mine, not ana's approval");
    assert_eq!(items[0].author.as_deref(), Some("keppelen"));
    assert_eq!(
        (
            items[1].since.commits,
            items[1].since.replies,
            items[1].since.last_activity.clone()
        ),
        (0, 0, None),
        "nothing since the review"
    );
}

#[test]
fn an_answer_without_a_viewer_or_a_search_has_nothing() {
    assert_eq!(from_search(&json!({ "data": {} })), (None, Vec::new()));
    let (viewer, items) = from_search(&json!({ "data": { "viewer": { "login": "w" } } }));
    assert_eq!((viewer.as_deref(), items.len()), (Some("w"), 0));
}

/// The real search through the person's `gh`, to look at:
/// `cargo test -p dcc-tauri --lib live_reviewed_search -- --ignored --nocapture`.
#[test]
#[ignore = "calls GitHub through the installed gh"]
fn live_reviewed_search() {
    let answer = dcc_infra::forge::github::reviewed_pull_requests_json("github.com", None).unwrap();
    let (viewer, items) = from_search(&answer);
    println!("viewer: {viewer:?}");
    for item in items {
        println!(
            "{} #{} [{}] — {} commits, {} replies since {} (last {:?})",
            item.repository,
            item.number,
            item.state,
            item.since.commits,
            item.since.replies,
            item.since.reviewed_at,
            item.since.last_activity
        );
    }
}
