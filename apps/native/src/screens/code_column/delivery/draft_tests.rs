//! The ✨ over a fake desktop: it fills only its field, sends the other one
//! along, keeps the field on a failure and drops an answer once stopped.

use gpui::{Modifiers, TestAppContext};
use serde_json::json;

use super::draft::DraftField;
use super::tests::{PREPARE, open, requests, toasts};
use crate::test_support::git_checkout;

const DRAFT: &str = "/api/v1/workspaces/w-1/delivery/draft";
const SUGGESTION: &str = r#"{"subject":"chore: update a","body":null,"stagedFileCount":1,"stagedFingerprint":"fp-1","source":"heuristic-git-staged"}"#;

fn changed_checkout() -> tempfile::TempDir {
    let repo = git_checkout(&[("a.txt", "um\n")]);
    std::fs::write(repo.path().join("a.txt"), "dois\n").unwrap();
    repo
}

/// The commit preview open with the desktop's deterministic subject.
fn open_preview(cx: &mut gpui::VisualTestContext) {
    let button = cx
        .debug_bounds("delivery-commit")
        .expect("the delivery button");
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
}

#[gpui::test]
fn the_sparkles_write_the_commit_body_for_its_subject(cx: &mut TestAppContext) {
    let repo = changed_checkout();
    let answer = r#"{"text":"Troca um por dois."}"#.to_string();
    let (column, cx, setup) = open(
        cx,
        repo.path(),
        None,
        vec![(PREPARE, 200, SUGGESTION.into()), (DRAFT, 200, answer)],
    );
    open_preview(cx);
    column.update(cx, |column, cx| {
        column.toggle_draft(DraftField::CommitBody, cx);
        assert!(column.delivery.drafting.is_some());
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let preview = column.delivery.preview.as_ref().unwrap();
        assert_eq!(preview.body.read(cx).text(), "Troca um por dois.");
        assert_eq!(
            preview.subject.read(cx).text(),
            "chore: update a",
            "untouched"
        );
        assert!(column.delivery.drafting.is_none());
    });
    let requests = requests(setup);
    assert_eq!(requests[1].0, DRAFT);
    assert_eq!(
        requests[1].1,
        json!({ "field": "commitBody", "companion": "chore: update a" })
    );
}

#[gpui::test]
fn a_failed_draft_keeps_the_field_and_says_so(cx: &mut TestAppContext) {
    let repo = changed_checkout();
    let failure = r#"{"error":"provider claude is disabled"}"#.to_string();
    let (column, cx, setup) = open(
        cx,
        repo.path(),
        None,
        vec![(PREPARE, 200, SUGGESTION.into()), (DRAFT, 500, failure)],
    );
    open_preview(cx);
    column.update(cx, |column, cx| {
        column.toggle_draft(DraftField::CommitSubject, cx)
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let preview = column.delivery.preview.as_ref().unwrap();
        assert_eq!(preview.subject.read(cx).text(), "chore: update a");
        assert!(column.delivery.drafting.is_none());
    });
    assert!(
        toasts(cx)
            .iter()
            .any(|toast| toast == "Não deu para escrever com IA"),
        "{:?}",
        toasts(cx)
    );
    assert_eq!(requests(setup)[1].1["field"], "commitSubject");
}

#[gpui::test]
fn a_stopped_draft_leaves_the_field_alone(cx: &mut TestAppContext) {
    let repo = changed_checkout();
    let answer = r#"{"text":"chore: something else"}"#.to_string();
    let (column, cx, _setup) = open(
        cx,
        repo.path(),
        None,
        vec![(PREPARE, 200, SUGGESTION.into()), (DRAFT, 200, answer)],
    );
    open_preview(cx);
    column.update(cx, |column, cx| {
        column.toggle_draft(DraftField::CommitSubject, cx);
        // Another field waits for the one writing.
        column.toggle_draft(DraftField::CommitBody, cx);
        assert_eq!(
            column
                .delivery
                .drafting
                .as_ref()
                .map(|drafting| drafting.field),
            Some(DraftField::CommitSubject)
        );
        // The same ✨ again stops it.
        column.toggle_draft(DraftField::CommitSubject, cx);
        assert!(column.delivery.drafting.is_none());
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let preview = column.delivery.preview.as_ref().unwrap();
        assert_eq!(preview.subject.read(cx).text(), "chore: update a");
    });
}

#[gpui::test]
fn the_sparkles_write_the_request_description_for_its_title(cx: &mut TestAppContext) {
    let repo = changed_checkout();
    let answer = r#"{"text":"Troca o texto.\n\n- a.txt"}"#.to_string();
    let (column, cx, setup) = open(cx, repo.path(), None, vec![(DRAFT, 200, answer)]);
    column.update_in(cx, |column, window, cx| {
        column.open_request_dialog(false, window, cx)
    });
    cx.run_until_parked();
    column.update(cx, |column, cx| {
        let request = column.delivery.request.as_ref().unwrap();
        request
            .title
            .update(cx, |input, cx| input.set_text("Novo texto", cx));
        column.toggle_draft(DraftField::RequestBody, cx);
    });
    cx.run_until_parked();
    column.read_with(cx, |column, cx| {
        let request = column.delivery.request.as_ref().unwrap();
        assert_eq!(request.body.read(cx).text(), "Troca o texto.\n\n- a.txt");
        assert_eq!(request.title.read(cx).text(), "Novo texto");
    });
    let requests = requests(setup);
    assert_eq!(
        requests[0].1,
        json!({ "field": "requestBody", "companion": "Novo texto" })
    );
}
