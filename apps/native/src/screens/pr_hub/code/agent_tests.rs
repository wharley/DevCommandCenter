//! The reviewer in the hub against a fake desktop: the whole review it runs
//! from its panel on the code tab (on its own provider, with its effort),
//! the drafts it leaves, and "Preparar com agente" for a line and a reply.

use std::sync::Arc;

use dcc_core::domain::provider::ProviderDescriptor;
use gpui::{Modifiers, TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use crate::data::agents::AgentRecord;
use crate::data::desktop_api::DesktopApi;
use crate::data::pr_review_jobs::{JobStatus, PrReviewJobs};
use crate::screens::pr_hub::PrHub;
use crate::screens::pr_hub::code::Tab;
use crate::screens::pr_hub::tests::item;
use crate::theme::{Theme, ThemePreference};

fn reviewer() -> AgentRecord {
    AgentRecord {
        projects: Vec::new(),
        monthly_token_limit: None,
        id: "rev".into(),
        name: "Revisor".into(),
        preset: Some("reviewer".into()),
        avatar: dcc_core::domain::agent::AgentAvatar {
            shape: "octopus".to_string(),
            accessory: "none".to_string(),
            color: "violet".into(),
            arms: 3,
            eyes: "round".into(),
        },
        provider_id: Some("codex".into()),
        model: Some("gpt".into()),
        effort: Some("xhigh".into()),
        kickoff_prompt: String::new(),
        role: String::new(),
        offer_prompt: String::new(),
        extra_instructions: String::new(),
        sessions: Vec::new(),
    }
}

fn provider(id: &str, read_only: bool) -> ProviderDescriptor {
    serde_json::from_value(json!({
        "id": id, "label": id.to_uppercase(), "description": "",
        "models": [{ "id": "gpt", "label": "GPT", "description": "", "recommended": true,
            "effortLevels": ["low", "medium", "high"] }],
        "capabilities": { "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
            "resumable": true, "experimental": false, "canBeDelegationTarget": true,
            "canRequestDelegation": true, "supportsReadOnlyDelegation": read_only,
            "supportsEditDelegation": true },
        "health": "Healthy", "stable": true
    }))
    .unwrap()
}

#[gpui::test]
fn the_reviewer_reviews_from_its_panel_and_prepares(cx: &mut TestAppContext) {
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
        "files": [{ "path": "src/login.rs", "status": "modified", "additions": 1, "deletions": 1,
            "patch": "@@ -1,2 +1,2 @@\n fn login() {\n-    old();\n+    new();" }],
        "inlineComments": [{ "id": "c-1", "threadId": "t-1", "path": "src/login.rs", "line": 2,
            "side": "right", "body": "Por que?", "author": { "login": "bob" }, "resolved": false }],
        "reviewCapabilities": { "inlineComments": true, "approve": true, "requestChanges": true,
            "replyToThreads": true, "resolveThreads": true },
    })
    .to_string();
    let review = json!({ "response": "```json\n{\"summary\":\"Um ponto.\",\"comments\":[\
        {\"path\":\"src/login.rs\",\"line\":2,\"side\":\"right\",\"body\":\"new() pode falhar\"},\
        {\"path\":\"src/login.rs\",\"line\":40,\"side\":\"right\",\"body\":\"fora do patch\"}]}\n```" })
    .to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail),
        ("/api/v1/pull-request-hub/review-agent", 200, review),
        (
            "/api/v1/pull-request-hub/review-agent",
            200,
            r#"{"response":"{\"reply\":\"Renomeie\"}"}"#.into(),
        ),
        (
            "/api/v1/pull-request-hub/review-agent",
            200,
            r#"{"response":"{\"reply\":\"Porque sim\"}"}"#.into(),
        ),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.set_sources(
            Some(reviewer()),
            Arc::new(vec![
                provider("claude", true),
                provider("codex", true),
                provider("cursor", false),
            ]),
            (Some("claude".into()), Some("gpt".into())),
            cx,
        );
        hub.refresh(cx);
    });
    cx.run_until_parked();
    // Nothing floats over the pull request: the review starts from the
    // reviewer's button on the code tab.
    hub.update(cx, |hub, cx| hub.set_tab(Tab::Code, cx));
    cx.run_until_parked();
    for id in ["pr-review-agent", "pr-review-generate"] {
        let bounds = cx.debug_bounds(id).unwrap();
        cx.simulate_click(bounds.center(), Modifiers::none());
        cx.run_until_parked();
    }
    // The review ran and left its draft (the line outside the patch dropped).
    let job = cx.update(|_, cx| PrReviewJobs::job(cx, "pr-1")).unwrap();
    assert_eq!((job.status, job.seen), (JobStatus::Done, true));
    assert_eq!(job.label, "acme/app#1");
    assert!(cx.debug_bounds("pr-review-result").is_some());
    assert!(cx.debug_bounds("pr-draft-src/login.rs:right:2").is_some());
    let body = hub.read_with(cx, |hub, cx| hub.review.body.read(cx).text().to_string());
    assert_eq!(body, "Um ponto.");
    // "Preparar com agente" on a line, then on the conversation.
    let bounds = cx.debug_bounds("pr-line-plus-2").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let bounds = cx.debug_bounds("pr-line-prepare").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let draft = hub.read_with(cx, |hub, cx| hub.review.draft.read(cx).text().to_string());
    assert_eq!(draft, "Renomeie");
    assert!(cx.debug_bounds("pr-line-editor").is_some());
    let bounds = cx.debug_bounds("pr-thread-prepare-c-1").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
    let seen = desktop.join().unwrap();
    cx.run_until_parked();
    let (reply, replying) = hub.read_with(cx, |hub, cx| {
        (
            hub.review.reply.read(cx).text().to_string(),
            hub.review.replying_to.clone(),
        )
    });
    assert_eq!(
        (reply.as_str(), replying.as_deref()),
        ("Porque sim", Some("c-1"))
    );
    let runs: Vec<serde_json::Value> = seen
        .iter()
        .filter(|(path, _)| path.ends_with("/review-agent"))
        .map(|(_, body)| serde_json::from_str(body).unwrap())
        .collect();
    assert_eq!(runs.len(), 3);
    // The whole review: as the reviewer, on its own provider and model, its
    // effort clamped to the model's.
    assert_eq!(runs[0]["agentId"], "rev");
    assert_eq!(runs[0]["providerId"], "codex");
    assert_eq!(runs[0]["model"], "gpt");
    assert_eq!(runs[0]["effort"], "high");
    assert_eq!(runs[0]["workingDirectory"], "/repo");
    assert!(
        runs[0]["prompt"]
            .as_str()
            .unwrap()
            .contains("### src/login.rs")
    );
    // A line and a reply: no reviewer, no effort.
    assert_eq!(runs[1]["agentId"], serde_json::Value::Null);
    assert_eq!(runs[1]["effort"], serde_json::Value::Null);
    assert!(
        runs[1]["prompt"]
            .as_str()
            .unwrap()
            .contains("Selected left line 2:     old();")
    );
    assert!(
        runs[2]["prompt"]
            .as_str()
            .unwrap()
            .contains("Comment by @bob:\nPor que?")
    );
}

#[gpui::test]
fn the_reviewer_sits_out_without_a_read_only_provider(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (hub, cx) = cx.add_window_view(PrHub::new);
    let target = hub.update(cx, |hub, cx| {
        hub.set_sources(
            Some(reviewer()),
            Arc::new(vec![provider("cursor", false)]),
            (None, None),
            cx,
        );
        hub.review_target().map(|target| target.provider.id.0)
    });
    assert_eq!(target, None);
    // Without its own provider it follows the composer's pick, then the first.
    let picked = hub.update(cx, |hub, cx| {
        let mut plain = reviewer();
        plain.provider_id = None;
        hub.set_sources(
            Some(plain),
            Arc::new(vec![provider("claude", true), provider("codex", true)]),
            (Some("codex".into()), None),
            cx,
        );
        let target = hub.review_target().unwrap();
        let run = hub.agent_run("/repo", "p".into(), true).unwrap();
        (target.provider.id.0, run.effort, run.agent_id)
    });
    assert_eq!(picked, ("codex".into(), None, Some("rev".into())));
}

#[gpui::test]
fn the_panels_pick_wins_over_the_reviewers_settings(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
    });
    let (hub, cx) = cx.add_window_view(PrHub::new);
    let run = |hub: &PrHub| {
        let run = hub.agent_run("/repo", "p".into(), true).unwrap();
        let line = hub.agent_run("/repo", "p".into(), false).unwrap();
        (run.provider_id, run.effort, run.agent_id, line.effort)
    };
    // Its own provider, with its effort clamped to the model's levels.
    let own = hub.update(cx, |hub, cx| {
        hub.set_sources(
            Some(reviewer()),
            Arc::new(vec![provider("claude", true), provider("codex", true)]),
            (Some("claude".into()), None),
            cx,
        );
        run(hub)
    });
    assert_eq!(
        own,
        (
            "codex".into(),
            Some("high".into()),
            Some("rev".into()),
            None
        )
    );
    // The panel's pick runs instead, still as the reviewer.
    let picked = hub.update(cx, |hub, cx| {
        let mut pick = crate::data::pr_review_pick::ReviewPick::default();
        pick.pick_model("claude", "gpt");
        pick.effort = Some("low".into());
        pick.account = Some(String::new());
        hub.set_review_pick(pick, cx);
        run(hub)
    });
    assert_eq!(
        picked,
        (
            "claude".into(),
            Some("low".into()),
            Some("rev".into()),
            None
        )
    );
    let account = hub.read_with(cx, |hub, _| hub.review_target().unwrap().account);
    assert_eq!(account, None, "\"\" is the system login");
}

#[gpui::test]
fn a_pull_request_asked_from_the_agents_page_opens_on_its_code(cx: &mut TestAppContext) {
    use crate::test_support::fake_desktop_routes;
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
    });
    let list = json!({
        "items": [item("pr-1", 1, "Fix login", true, false), item("pr-2", 2, "Docs", false, true)],
        "warnings": [],
    })
    .to_string();
    let detail = json!({ "files": [] }).to_string();
    let (port, desktop) = fake_desktop_routes(vec![
        ("/api/v1/pull-request-hub/list", 200, list),
        // The summary, then the code tab's read with the patches.
        ("/api/v1/pull-request-hub/detail", 200, detail.clone()),
        ("/api/v1/pull-request-hub/detail", 200, detail),
    ]);
    let (hub, cx) = cx.add_window_view(PrHub::new);
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    hub.update(cx, |hub, cx| {
        hub.api = Some(DesktopApi::new(port, "k"));
        hub.filter = crate::data::pr_hub::Filter::Mine;
        PrReviewJobs::request_open(cx, "pr-1");
        hub.take_requested(cx);
        hub.refresh(cx);
    });
    cx.run_until_parked();
    let _ = desktop.join().unwrap();
    cx.run_until_parked();
    let (selected, tab, filter, requested) = hub.read_with(cx, |hub, _| {
        (
            hub.selected.clone(),
            hub.tab,
            hub.filter,
            hub.requested.clone(),
        )
    });
    assert_eq!(selected.as_deref(), Some("pr-1"));
    assert_eq!(tab, Tab::Code);
    assert_eq!(filter, crate::data::pr_hub::Filter::All);
    assert_eq!(requested, None);
    assert!(cx.debug_bounds("pr-code-empty").is_some());
}
