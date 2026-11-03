use super::*;
use crate::data::pr_hub::SinceViewerReview;

fn pr(number: u32, commits: u32, last: Option<&str>) -> Reviewed {
    Reviewed {
        url: format!("https://github.com/acme/app/pull/{number}"),
        number,
        repository: "acme/app".into(),
        title: format!("PR {number}"),
        author: Some("daniel".into()),
        state: "COMMENTED".into(),
        since: SinceViewerReview {
            reviewed_at: "2026-11-03T12:00:00Z".into(),
            commits,
            replies: 0,
            last_activity: last.map(str::to_string),
        },
    }
}

#[test]
fn only_what_came_after_my_review_is_news() {
    let news = PrNews::in_memory();
    assert!(news.is_new(&pr(1, 2, Some("2026-11-03T14:00:00Z"))));
    assert!(!news.is_new(&pr(2, 0, None)), "reviewed, nothing since");
}

#[test]
fn the_first_search_announces_nothing_and_later_news_once() {
    let mut news = PrNews::in_memory();
    let old = pr(1, 1, Some("2026-11-03T13:00:00Z"));
    assert!(news.read(vec![old]).is_empty(), "old news");
    assert_eq!(news.count(), 1, "still counted until opened");
    // A new commit on it, and news on another one.
    let newer = pr(1, 2, Some("2026-11-03T15:00:00Z"));
    let other = pr(2, 1, Some("2026-11-03T15:30:00Z"));
    let announced = news.read(vec![newer.clone(), other.clone()]);
    let numbers: Vec<u32> = announced.iter().map(|pr| pr.number).collect();
    assert_eq!(numbers, [1, 2]);
    assert!(news.read(vec![newer, other]).is_empty(), "never twice");
}

#[test]
fn opening_it_sees_it_until_something_else_comes() {
    let mut news = PrNews::in_memory();
    let first = pr(1, 1, Some("2026-11-03T13:00:00Z"));
    news.read(vec![first.clone()]);
    assert!(news.mark_seen(&first.url));
    assert!(!news.is_new(&first));
    assert_eq!(news.count(), 0);
    assert!(!news.mark_seen(&first.url), "nothing left to see");
    let again = pr(1, 2, Some("2026-11-03T16:00:00Z"));
    news.read(vec![again.clone()]);
    assert!(news.is_new(&again));
}

#[test]
fn merged_pull_requests_are_forgotten_and_marks_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pr-review-seen.json");
    let seen = pr(1, 1, Some("2026-11-03T13:00:00Z"));
    let mut news = PrNews::load(path.clone());
    news.read(vec![seen.clone(), pr(2, 1, Some("2026-11-03T13:00:00Z"))]);
    news.mark_seen(&seen.url);
    // #2 merged: the search no longer lists it.
    news.read(vec![seen.clone()]);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("pull/2"), "{raw}");
    let mut restarted = PrNews::load(path);
    assert!(!restarted.is_new(&seen));
    // Started before: news after the restart is announced.
    let newer = pr(1, 2, Some("2026-11-03T18:00:00Z"));
    assert_eq!(restarted.read(vec![newer]).len(), 1);
}
