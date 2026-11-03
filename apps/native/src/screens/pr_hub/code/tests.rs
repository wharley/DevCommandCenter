//! The code tab against a fake desktop: the files and the patch, a comment
//! drafted on a line, a reply on a conversation's first comment, resolving
//! it, and the review sent with the draft.

use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use crate::data::desktop_api::DesktopApi;
use crate::data::pr_review::ReviewEvent;
use crate::screens::pr_hub::PrHub;
use crate::screens::pr_hub::code::Tab;
use crate::screens::pr_hub::tests::item;
use crate::theme::{Theme, ThemePreference};

#[gpui::test]
fn the_code_tab_drafts_replies_resolves_and_sends(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "comments": [], "checks": [],
        "files": [
            { "path": "src/login.rs", "status": "modified", "additions": 2, "deletions": 1,
              "patch": "@@ -1,2 +1,3 @@\n fn login() {\n-    old();\n+    new();\n+    more();" },
            { "path": "README.md", "status": "added", "additions": 1, "deletions": 0, "patch": null },
        ],
        "inlineComments": [
            { "id": "c-1", "threadId": "t-1", "path": "src/login.rs", "line": 2, "side": "right",
              "body": "Por que new?", "author": { "login": "bob" }, "resolved": false },
            { "id": "c-2", "threadId": "t-1", "path": "src/login.rs", "line": 2, "side": "right",
              "body": "Explico.", "author": { "login": "ana" }, "resolved": false },
        ],
        "reviewCapabilities": { "inlineComments": true, "approve": true, "requestChanges": true,
            "replyToThreads": true, "resolveThreads": true },
        "mergeCapabilities": null,
    })
    .to_string();
    let detail_route = || ("/api/v1/pull-request-hub/detail", 200, detail.clone());
    let sent = json!({ "submitted": true, "url": null, "submittedCommentCount": 1,
        "bodySubmitted": false, "decisionSubmitted": true, "warning": null })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list.clone()),
        detail_route(),
        detail_route(),
        (
            "/api/v1/pull-request-hub/reply-thread",
            200,
            r#"{"submitted":true}"#.into(),
        ),
        detail_route(),
        (
            "/api/v1/pull-request-hub/resolve-thread",
            200,
            r#"{"resolved":true}"#.into(),
        ),
        detail_route(),
        ("/api/v1/pull-request-hub/submit-review", 200, sent),
        detail_route(),
        detail_route(),
        ("/api/v1/pull-request-hub/list", 200, list),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("pr-tab-code").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hub.read_with(cx, |hub, _| hub.tab), Tab::Code);
    assert!(cx.debug_bounds("pr-code-files").is_some());
    assert!(cx.debug_bounds("pr-file-1").is_some(), "both files");
    // The list says which file holds conversations (one thread of two comments).
    assert!(
        cx.debug_bounds("pr-file-talks-0").is_some(),
        "login.rs talks"
    );
    assert!(
        cx.debug_bounds("pr-file-talks-1").is_none(),
        "README has none"
    );
    assert!(cx.debug_bounds("pr-review-footer").is_some());
    assert!(
        cx.debug_bounds("pr-thread-c-1").is_some(),
        "the conversation on line 2"
    );
    // A comment on the deleted line (row 2: the hunk, then the context).
    let bounds = cx.debug_bounds("pr-line-plus-2").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-line-editor").is_some());
    hub.update(cx, |hub, cx| {
        let draft = hub.review.draft.clone();
        draft.update(cx, |area, cx| area.set_text("  Nit: nome  ", cx));
    });
    let bounds = cx.debug_bounds("pr-line-save").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-line-editor").is_none());
    assert!(cx.debug_bounds("pr-draft-src/login.rs:left:2").is_some());
    assert!(cx.debug_bounds("pr-review-pending").is_some());
    // A reply on the second comment goes to the conversation's first.
    let bounds = cx.debug_bounds("pr-thread-reply-c-2").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-thread-reply-field").is_some());
    hub.update(cx, |hub, cx| {
        let reply = hub.review.reply.clone();
        reply.update(cx, |area, cx| area.set_text("Valeu", cx));
    });
    let bounds = cx.debug_bounds("pr-thread-reply-send").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-thread-reply-field").is_none());
    let bounds = cx.debug_bounds("pr-thread-resolve-c-1").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    // Approve, with the draft.
    let bounds = cx.debug_bounds("pr-review-event-approve").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        hub.read_with(cx, |hub, _| hub.review.event),
        ReviewEvent::Approve
    );
    let bounds = cx.debug_bounds("pr-review-send").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let (drafts, event, submitted) = hub.read_with(cx, |hub, _| {
        (
            hub.review.drafts.len(),
            hub.review.event,
            hub.review.submitted,
        )
    });
    assert_eq!((drafts, event, submitted), (0, ReviewEvent::Comment, true));
    assert!(cx.debug_bounds("pr-review-pending").is_none());
    let body = |path: &str| {
        let (_, body) = seen
            .iter()
            .find(|(request, _)| request.ends_with(path))
            .unwrap();
        serde_json::from_str::<serde_json::Value>(body).unwrap()
    };
    let reply = body("/reply-thread");
    assert_eq!(reply["commentId"], "c-1");
    assert_eq!(reply["threadId"], "t-1");
    assert_eq!(reply["body"], "Valeu");
    let resolve = body("/resolve-thread");
    assert_eq!(
        (resolve["threadId"].as_str(), resolve["resolved"].as_bool()),
        (Some("t-1"), Some(true))
    );
    let review = body("/submit-review");
    assert_eq!(review["event"], "approve");
    assert_eq!(review["body"], serde_json::Value::Null);
    assert_eq!(review["forgeLogin"], "ana");
    assert_eq!(
        review["comments"],
        json!([{ "path": "src/login.rs", "body": "Nit: nome", "line": 2, "side": "left" }])
    );
    // The second file has no patch: no line to comment on.
    hub.update(cx, |hub, cx| hub.choose_file("README.md".into(), cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-line-plus-1").is_none());
}

/// After the author pushed a fix and answered on the pull request: the
/// strip says so, the outdated thread stands apart with its old code (not
/// over the new line), the untouched one keeps its line, and "Ver resposta"
/// goes to the summary where the answer is.
#[gpui::test]
fn the_code_tab_says_what_changed_since_the_review(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "checks": [],
        "comments": [{ "id": "fix", "body": "Corrigi tudo.", "author": { "login": "bob" },
            "createdAt": "2026-11-01T19:57:50Z" }],
        "reviews": [{ "author": { "login": "ana" }, "state": "COMMENTED",
            "submittedAt": "2026-11-01T17:51:51Z" }],
        "commits": [
            { "oid": "a", "headline": "feat", "committedDate": "2026-11-01T17:44:21Z" },
            { "oid": "b", "headline": "fix", "committedDate": "2026-11-01T19:57:28Z" },
        ],
        "files": [{ "path": "src/login.rs", "status": "modified", "additions": 2, "deletions": 1,
            "patch": "@@ -1,2 +1,3 @@\n fn login() {\n-    old();\n+    new();\n+    more();" }],
        "inlineComments": [
            { "id": "c-old", "threadId": "t-old", "path": "src/login.rs", "line": 2, "side": "right",
              "body": "TTL renovado", "author": { "login": "ana" }, "resolved": false,
              "createdAt": "2026-11-01T17:51:50Z", "outdated": true,
              "diffHunk": "@@ -1 +1,2 @@\n+    saveCache();" },
            { "id": "c-same", "threadId": "t-same", "path": "src/login.rs", "line": 3, "side": "right",
              "body": "Faixa larga", "author": { "login": "ana" }, "resolved": false,
              "createdAt": "2026-11-01T17:51:50Z" },
        ],
        "reviewCapabilities": { "inlineComments": true, "approve": true, "requestChanges": true,
            "replyToThreads": true, "resolveThreads": true },
        "mergeCapabilities": null,
    })
    .to_string();
    let detail_route = || ("/api/v1/pull-request-hub/detail", 200, detail.clone());
    let (port, _desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        detail_route(),
        detail_route(),
        detail_route(),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("pr-tab-code").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.debug_bounds("pr-since-review").is_some());
    let apart = cx.debug_bounds("pr-outdated-threads").unwrap();
    let old = cx.debug_bounds("pr-thread-c-old").unwrap();
    assert!(
        apart.contains(&old.center()),
        "the outdated thread stands apart"
    );
    let same = cx.debug_bounds("pr-thread-c-same").unwrap();
    assert!(
        !apart.contains(&same.center()),
        "the current one stays on its line"
    );
    assert!(same.top() > old.top());
    let bounds = cx.debug_bounds("pr-since-open-reply").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(hub.read_with(cx, |hub, _| hub.tab), Tab::Summary);
}

/// The Reviewer's review is long: the field scrolls inside and its send
/// button stays in a short window.
#[gpui::test]
fn a_long_review_keeps_its_send_button_in_the_window(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list =
        json!({ "items": [item("pr-1", 1, "Fix login", true, false)], "warnings": [] }).to_string();
    let detail = json!({
        "body": null, "comments": [], "checks": [], "inlineComments": [],
        "files": [
            { "path": "src/login.rs", "status": "modified", "additions": 1, "deletions": 1,
              "patch": "@@ -1,1 +1,1 @@\n-    old();\n+    new();" },
        ],
        "reviewCapabilities": { "inlineComments": true, "approve": true, "requestChanges": true,
            "replyToThreads": true, "resolveThreads": true },
        "mergeCapabilities": null,
    })
    .to_string();
    let detail_route = || ("/api/v1/pull-request-hub/detail", 200, detail.clone());
    let (port, _desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        detail_route(),
        detail_route(),
        detail_route(),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1100.0), px(640.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.refresh(cx);
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("pr-tab-code").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    hub.update(cx, |hub, cx| {
        let body = hub.review.body.clone();
        body.update(cx, |area, cx| {
            area.set_text(&"Uma linha da revisão do Revisor.\n".repeat(80), cx)
        });
    });
    cx.run_until_parked();
    let field = cx.debug_bounds("pr-review-body").unwrap();
    assert!(field.size.height <= px(220.0), "{:?}", field.size);
    let send = cx.debug_bounds("pr-review-send").unwrap();
    assert!(send.bottom() <= px(640.0), "send at {:?}", send);
    assert!(
        cx.debug_bounds("pr-code-files").is_some(),
        "the files stay above"
    );
}
