//! "Resolver com agente" against a fake desktop: the run asked as one
//! read-only call (no conversation started), an answer out of contract
//! shown as a failure, the answer put in the editor and offered block by
//! block (accept, refuse, discard), and a run the person stopped.

use dcc_core::domain::merge_conflict::{WorkspaceGitConflictEntry, WorkspaceGitConflictOperation};
use gpui::{TestAppContext, WindowAppearance, px, size};
use serde_json::json;

use super::agent::{AgentRun, AgentStatus};
use super::tests::{checks, click, source, state, text};
use super::{AgentPick, Busy, MergeResolver, MergeResolverRequest};
use crate::data::conversation::ThreadStart;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::test_support::fake_desktop_routes;
use crate::theme::{Theme, ThemePreference};

const A: &str = "<<<<<<< HEAD\nmine a\n=======\ntheirs a\n>>>>>>> origin/main\n";
const B: &str = "<<<<<<< HEAD\nmine b\n=======\ntheirs b\n>>>>>>> origin/main\n";

/// A file with two blocks and enough between them to tell them apart.
fn two_blocks() -> String {
    format!("top\n{A}fn between() {{ body(); }}\n{B}bottom\n")
}

fn entry() -> WorkspaceGitConflictEntry {
    WorkspaceGitConflictEntry {
        result: text(Some(&two_blocks())),
        ..source()
    }
}

type Opened<'a> = (
    gpui::Entity<MergeResolver>,
    &'a mut gpui::VisualTestContext,
    std::thread::JoinHandle<Vec<(String, String)>>,
);

fn open<'a>(cx: &'a mut TestAppContext, extra: Vec<(&'static str, u16, String)>) -> Opened<'a> {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
        crate::screens::file_view::bind_keys(cx);
    });
    let merge = WorkspaceGitConflictOperation::Merge;
    let mut routes = vec![
        (
            "/api/v1/git/conflict-state",
            200,
            state(merge, vec![entry()]),
        ),
        ("/api/v1/git/validation-config", 200, checks(&[])),
    ];
    routes.extend(extra);
    let (port, server) = fake_desktop_routes(routes);
    let request = MergeResolverRequest {
        start: false,
        root: "/tmp/checkout".into(),
        task_id: Some("w-1".into()),
        base_branch: Some("main".into()),
        forge_login: None,
        workspace_label: None,
        agent: Some(AgentPick {
            start: ThreadStart {
                workspace_id: "w-1".into(),
                project_id: "p".into(),
                additional_workspace_ids: Vec::new(),
                name: Some("Pagamentos".into()),
                auto_named: false,
            },
            provider_id: "claude_code".into(),
            model: Some("claude-sonnet-5-5".into()),
        }),
    };
    let (resolver, cx) = cx.add_window_view(move |_, cx| {
        MergeResolver::new(request, cx).with_endpoint(DesktopApi::new(port, "k"), cx)
    });
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    cx.run_until_parked();
    (resolver, cx, server)
}

/// The agent's answer for the two-block file, in this token's envelope.
fn answer(token: &str, resolved: &str) -> String {
    // The whole file, as answers used to come (still taken).
    let body = json!({
        "resolvedContent": resolved,
        "explanation": "Juntei os dois lados.",
        "blockNotes": ["Fiquei com os dois em a.", "Fiquei com a base em b."],
    });
    format!(
        "<DCC_MERGE_RESOLUTION token=\"{token}\">{body}</DCC_MERGE_RESOLUTION token=\"{token}\">"
    )
}

/// A run under way, as `resolve_with_agent` leaves it, without the call.
fn running(resolver: &gpui::Entity<MergeResolver>, cx: &mut gpui::VisualTestContext) {
    resolver.update(cx, |resolver, cx| {
        resolver.busy = Some(Busy::Agent);
        resolver.agent = Some(AgentRun {
            status: AgentStatus::Analyzing,
            error: None,
            token: "tok".into(),
            raw: None,
            raw_open: false,
            path: "src/a.rs".into(),
            queue: Vec::new(),
            total: 1,
            failed: Vec::new(),
        });
        cx.notify();
    });
    cx.run_until_parked();
}

const RESOLVED: &str = "top\nboth a\nfn between() { body(); }\ntheirs b\nbottom\n";

#[gpui::test]
fn the_run_is_one_read_only_call_and_an_answer_out_of_contract_fails(cx: &mut TestAppContext) {
    let (resolver, cx, server) = open(
        cx,
        vec![(
            "/api/v1/git/conflict-agent",
            200,
            json!({ "response": "I could not do it." }).to_string(),
        )],
    );
    click(cx, "merge-resolve-with-agent");
    let calls = server.join().unwrap();
    cx.run_until_parked();
    // No conversation was started: one call, on the composer's pick.
    assert!(!calls.iter().any(|(line, _)| line.contains("/sessions")));
    let (_, sent) = calls
        .iter()
        .find(|(line, _)| line.contains("/git/conflict-agent"))
        .expect("the run was asked");
    let sent: serde_json::Value = serde_json::from_str(sent).unwrap();
    assert_eq!(sent["workspaceRoot"], "/tmp/checkout");
    assert_eq!(sent["providerId"], "claude_code");
    assert_eq!(sent["model"], "claude-sonnet-5-5");
    assert!(sent["prompt"].as_str().unwrap().contains("mine a"));
    assert!(!sent["runId"].as_str().unwrap().is_empty());
    // Out of contract: said so, nothing put in the file, editing again.
    assert!(cx.debug_bounds("merge-agent-error").is_some());
    resolver.read_with(cx, |resolver, cx| {
        assert_eq!(resolver.agent.as_ref().unwrap().status, AgentStatus::Failed);
        assert!(resolver.proposal.is_none() && resolver.busy.is_none());
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(open.buffer, two_blocks());
        assert!(!open.editor.read(cx).read_only);
    });
    // No conversation to look it up in: the answer itself can be read.
    assert!(cx.debug_bounds("merge-agent-answer").is_none());
    click(cx, "merge-agent-show-answer");
    assert!(cx.debug_bounds("merge-agent-answer").is_some());
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(
            resolver.agent.as_ref().unwrap().raw.as_deref(),
            Some("I could not do it.")
        );
    });
    click(cx, "merge-agent-dismiss");
    assert!(cx.debug_bounds("merge-agent-run").is_none());
}

#[gpui::test]
fn the_agent_answers_the_blocks_and_the_file_is_put_together_here(cx: &mut TestAppContext) {
    let (resolver, cx, _server) = open(cx, vec![]);
    running(&resolver, cx);
    let body = json!({
        "blocks": [
            { "resolved": "both a\n", "note": "Fiquei com os dois em a." },
            { "resolved": "theirs b", "note": "Fiquei com a base em b." },
        ],
        "explanation": "Juntei os dois lados.",
    });
    let answer =
        format!("<DCC_MERGE_RESOLUTION token=\"tok\">{body}</DCC_MERGE_RESOLUTION token=\"tok\">");
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer), &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    resolver.read_with(cx, |resolver, _| {
        let open = resolver.open.as_ref().unwrap();
        // Only the blocks changed; what is between them is the file's own.
        assert_eq!(open.buffer, RESOLVED);
        let proposal = resolver.proposal.as_ref().unwrap();
        assert_eq!(proposal.blocks.len(), 2);
        assert_eq!(
            proposal.blocks[1].note.as_deref(),
            Some("Fiquei com a base em b.")
        );
        assert_eq!(proposal.blocks[1].proposed, "theirs b\n");
    });
    assert!(cx.debug_bounds("merge-proposal-accept-1").is_some());
}

#[gpui::test]
fn the_answer_is_offered_block_by_block_in_the_file(cx: &mut TestAppContext) {
    let (resolver, cx, _server) = open(cx, vec![]);
    running(&resolver, cx);
    assert!(cx.debug_bounds("merge-agent-cancel").is_some());
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer("tok", RESOLVED)), &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    // The whole answer is in the file, and each block is offered where it is.
    resolver.read_with(cx, |resolver, _| {
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(open.buffer, RESOLVED);
        let proposal = resolver.proposal.as_ref().unwrap();
        assert_eq!(proposal.blocks.len(), 2);
        assert_eq!(
            proposal.blocks[0].note.as_deref(),
            Some("Fiquei com os dois em a.")
        );
        assert_eq!(proposal.offered(&open.buffer), 2);
        assert!(resolver.agent.is_none() && resolver.busy.is_none());
    });
    assert!(cx.debug_bounds("merge-proposal").is_some());
    assert!(cx.debug_bounds("merge-proposal-accept-0").is_some());
    assert!(cx.debug_bounds("merge-proposal-refuse-1").is_some());
    // What each block took out shows above what the agent wrote, and is
    // not part of the file.
    assert!(cx.debug_bounds("file-ghost-line").is_some());
    resolver.read_with(cx, |resolver, cx| {
        let open = resolver.open.as_ref().unwrap();
        let editor = open.editor.read(cx);
        assert_eq!(editor.ghosts.len(), 2);
        assert_eq!(
            editor.ghosts[0]
                .lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()[1..],
            ["mine a", "origin/main · branch base", "theirs a"][..]
        );
        assert_eq!(editor.text(), RESOLVED, "the text has no ghost");
    });
    // The bar's arrows and each block's note take the editor to it.
    assert!(cx.debug_bounds("merge-hunk-position").is_some());
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.proposal.as_ref().unwrap().at, 0, "on the first");
    });
    click(cx, "merge-proposal-note-1");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.proposal.as_ref().unwrap().at, 1);
    });
    click(cx, "merge-hunk-previous");
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(resolver.proposal.as_ref().unwrap().at, 0);
    });
    // Refusing the second puts its conflict back, to resolve by hand.
    click(cx, "merge-proposal-refuse-1");
    resolver.read_with(cx, |resolver, _| {
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(
            open.buffer,
            format!("top\nboth a\nfn between() {{ body(); }}\n{B}bottom\n")
        );
        assert_eq!(open.hunks().len(), 1);
    });
    assert!(cx.debug_bounds("merge-accept-both-0").is_some(), "by hand");
    // Accepting the first leaves nothing to decide: the proposal goes.
    click(cx, "merge-proposal-accept-0");
    resolver.read_with(cx, |resolver, _| assert!(resolver.proposal.is_none()));
    assert!(cx.debug_bounds("merge-proposal").is_none());
}

#[gpui::test]
fn a_proposal_is_discarded_whole_or_kept_and_an_edited_block_is_the_persons(
    cx: &mut TestAppContext,
) {
    let (resolver, cx, _server) = open(cx, vec![]);
    running(&resolver, cx);
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer("tok", RESOLVED)), &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    // "Descartar": every block back to its conflict.
    click(cx, "merge-proposal-discard");
    resolver.read_with(cx, |resolver, _| {
        assert!(resolver.proposal.is_none());
        assert_eq!(resolver.open.as_ref().unwrap().buffer, two_blocks());
    });
    // Asked again; the text outside the blocks changed too: one proposal
    // for the file, kept or discarded whole.
    running(&resolver, cx);
    let rewritten = "TOP\nboth a\nfn other() {}\ntheirs b\nbottom\n";
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer("tok", rewritten)), &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    resolver.read_with(cx, |resolver, _| {
        let proposal = resolver.proposal.as_ref().unwrap();
        assert!(proposal.blocks.is_empty());
        assert_eq!(resolver.open.as_ref().unwrap().buffer, rewritten);
    });
    assert!(cx.debug_bounds("merge-proposal-accept-0").is_none());
    click(cx, "merge-proposal-accept-all");
    resolver.read_with(cx, |resolver, _| {
        assert!(resolver.proposal.is_none());
        assert_eq!(resolver.open.as_ref().unwrap().buffer, rewritten);
    });
}

#[gpui::test]
fn a_stopped_run_leaves_the_file_as_it_was(cx: &mut TestAppContext) {
    let (resolver, cx, server) = open(
        cx,
        vec![(
            "/api/v1/git/conflict-agent/cancel",
            200,
            r#"{"stopped":true}"#.into(),
        )],
    );
    running(&resolver, cx);
    click(cx, "merge-agent-cancel");
    let calls = server.join().unwrap();
    cx.run_until_parked();
    let (_, sent) = calls
        .iter()
        .find(|(line, _)| line.contains("/conflict-agent/cancel"))
        .expect("the stop was sent");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(sent).unwrap()["runId"],
        "tok"
    );
    resolver.read_with(cx, |resolver, _| {
        assert_eq!(
            resolver.agent.as_ref().unwrap().status,
            AgentStatus::Cancelling
        );
    });
    // The run comes back stopped: no failure shown, nothing changed.
    resolver.update(cx, |resolver, cx| {
        let stopped = Err(ApiError::Failed("The run was cancelled.".into()));
        resolver.finish_agent(stopped, &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    resolver.read_with(cx, |resolver, _| {
        assert!(resolver.agent.is_none() && resolver.busy.is_none());
        assert!(resolver.proposal.is_none());
        assert_eq!(resolver.open.as_ref().unwrap().buffer, two_blocks());
    });
    assert!(cx.debug_bounds("merge-agent-run").is_none());
}

#[gpui::test]
fn every_file_is_asked_in_turn_and_each_answer_waits_for_its_file(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::Toaster::init(cx);
        crate::screens::file_view::bind_keys(cx);
    });
    let other = WorkspaceGitConflictEntry {
        path: "src/b.rs".into(),
        ..entry()
    };
    let merge = WorkspaceGitConflictOperation::Merge;
    let (port, _server) = fake_desktop_routes(vec![
        (
            "/api/v1/git/conflict-state",
            200,
            state(merge, vec![entry(), other]),
        ),
        ("/api/v1/git/validation-config", 200, checks(&[])),
    ]);
    let request = MergeResolverRequest {
        start: false,
        root: "/tmp/checkout".into(),
        task_id: Some("w-1".into()),
        base_branch: Some("main".into()),
        forge_login: None,
        workspace_label: None,
        agent: None,
    };
    let (resolver, cx) = cx.add_window_view(move |_, cx| {
        MergeResolver::new(request, cx).with_endpoint(DesktopApi::new(port, "k"), cx)
    });
    cx.simulate_resize(size(px(1400.0), px(900.0)));
    cx.run_until_parked();
    // A run over both files, on the first (the open one).
    resolver.update(cx, |resolver, cx| {
        resolver.busy = Some(Busy::Agent);
        resolver.agent = Some(AgentRun {
            status: AgentStatus::Analyzing,
            error: None,
            token: "tok".into(),
            raw: None,
            raw_open: false,
            path: "src/a.rs".into(),
            queue: vec!["src/b.rs".into()],
            total: 2,
            failed: Vec::new(),
        });
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("merge-agent-file").is_some(), "which file");
    // The open file's answer is offered at once; the run goes on to the next.
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer("tok", RESOLVED)), &two_blocks(), "tok", cx)
    });
    cx.run_until_parked();
    let token = resolver.read_with(cx, |resolver, _| {
        assert!(resolver.proposal.is_some());
        let run = resolver.agent.as_ref().expect("still running");
        assert_eq!((run.path.as_str(), run.position()), ("src/b.rs", 2));
        run.token.clone()
    });
    // The other file's answer is kept, not put anywhere yet.
    resolver.update(cx, |resolver, cx| {
        resolver.finish_agent(Ok(answer(&token, RESOLVED)), &two_blocks(), &token, cx)
    });
    cx.run_until_parked();
    resolver.read_with(cx, |resolver, _| {
        assert!(resolver.agent.is_none() && resolver.busy.is_none());
        assert!(resolver.answers.contains_key("src/b.rs"));
        assert_eq!(resolver.open.as_ref().unwrap().entry.path, "src/a.rs");
    });
    // Opening that file offers what was kept for it.
    resolver.update(cx, |resolver, cx| {
        resolver.proposal = None;
        resolver.open_path("src/b.rs", cx)
    });
    cx.run_until_parked();
    resolver.read_with(cx, |resolver, _| {
        let open = resolver.open.as_ref().unwrap();
        assert_eq!(
            (open.entry.path.as_str(), open.buffer.as_str()),
            ("src/b.rs", RESOLVED)
        );
        assert_eq!(resolver.proposal.as_ref().unwrap().blocks.len(), 2);
        assert!(resolver.answers.is_empty());
    });
}
