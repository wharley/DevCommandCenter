use super::*;

#[test]
fn a_list_keeps_the_needed_fields_and_my_approvals() {
    let raw = json!([
        {"number": 1, "title": "A", "url": "u1", "state": "MERGED", "author": {"login": "ana"},
         "mergedAt": "2026-11-09T15:00:00Z", "mergeCommit": {"oid": "abc"}, "body": "long",
         "latestReviews": [{"author": {"login": "Me"}, "state": "APPROVED"}]},
        {"number": 2, "title": "B", "url": "u2", "state": "MERGED", "author": {"login": "bia"},
         "latestReviews": [{"author": {"login": "me"}, "state": "COMMENTED"}]}
    ]);
    let all = list_items(&raw, None);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0]["mergeCommit"], "abc");
    assert_eq!(all[0]["author"], "ana");
    assert!(all[0].get("body").is_none());
    let mine = list_items(&raw, Some("me"));
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["number"], 1);
}

#[test]
fn a_detail_sums_up_checks_and_reviews() {
    let raw = json!({
        "number": 7, "state": "OPEN", "mergeable": "MERGEABLE", "changedFiles": 3,
        "latestReviews": [{"author": {"login": "rui"}, "state": "APPROVED", "body": "x"}],
        "statusCheckRollup": [
            {"__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE"},
            {"__typename": "StatusContext", "context": "Vercel", "state": "PENDING"}
        ]
    });
    let view = pull_request_view(&raw);
    assert_eq!(view["reviews"][0]["author"], "rui");
    assert_eq!(
        view["checksSummary"],
        json!({"passed": 1, "failed": 1, "pending": 1})
    );
    assert_eq!(view["checks"][2]["name"], "Vercel");
    assert_eq!(view["changedFiles"], 3);
}

#[test]
fn the_answer_is_cut_to_fit() {
    let items: Vec<Value> = (0..100)
        .map(|n| json!({"n": n, "pad": "x".repeat(100)}))
        .collect();
    let (kept, cut) = fit(items.clone(), 1_200);
    assert!(kept.len() < 100 && kept.len() + cut == 100);
    assert!(Value::from(kept).to_string().len() <= 1_200);
    let (kept, cut) = fit(items, usize::MAX);
    assert_eq!((kept.len(), cut), (100, 0));
}

#[test]
fn the_newest_success_of_each_environment_is_compared() {
    let deployments = vec![
        json!({"environment": "Production", "sha": "b", "latestStatus": {"state": "in_progress"}}),
        json!({"environment": "Production", "sha": "a", "latestStatus": {"state": "success"}}),
        json!({"environment": "Preview", "sha": "c", "latestStatus": {"state": "success"}}),
        json!({"environment": "Production", "sha": "z", "latestStatus": {"state": "success"}}),
    ];
    assert_eq!(
        newest_successes(&deployments),
        [
            ("Production".to_string(), "a".to_string()),
            ("Preview".to_string(), "c".to_string())
        ]
    );
}
