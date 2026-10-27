//! The dialog against a GitHub of its own: it names the account, a report
//! is reviewed then published and leads "Meus feedbacks"; an unconfirmed
//! answer keeps the same request for "Verificar envio"; no account asks to
//! connect.

use std::sync::{Arc, Mutex};

use dcc_infra::feedback::{FeedbackContext, FeedbackInput, FeedbackIssue, FeedbackPage};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, WindowAppearance};

use super::{Feedback, Tab};
use crate::data::feedback::{Backend, Store};
use crate::theme::{Theme, ThemePreference};

#[derive(Default)]
struct FakeGitHub {
    no_account: bool,
    /// The first publications answer this error, in order.
    failures: Mutex<Vec<String>>,
    sent: Mutex<Vec<FeedbackInput>>,
}

impl Backend for FakeGitHub {
    fn context(&self) -> Result<FeedbackContext, String> {
        if self.no_account {
            return Err("FEEDBACK_AUTH_REQUIRED".into());
        }
        Ok(FeedbackContext {
            login: "ana".into(),
            version: "0.1.99".into(),
            platform: "macos".into(),
            architecture: "aarch64".into(),
        })
    }

    fn list(&self, _: &str, _: u32) -> Result<FeedbackPage, String> {
        Ok(FeedbackPage {
            issues: vec![FeedbackIssue {
                number: 3,
                title: "[Feature]: Older".into(),
                url: String::new(),
                category: "improvement".into(),
                state: "closed".into(),
                state_reason: Some("completed".into()),
                created_at: "2026-10-01T12:00:00Z".into(),
            }],
            has_next: false,
        })
    }

    fn create(&self, input: FeedbackInput) -> Result<FeedbackIssue, String> {
        self.sent.lock().unwrap().push(input.clone());
        let mut failures = self.failures.lock().unwrap();
        if !failures.is_empty() {
            return Err(failures.remove(0));
        }
        Ok(FeedbackIssue {
            number: 42,
            title: format!("[Bug]: {}", input.title),
            url: String::new(),
            category: input.category,
            state: "open".into(),
            state_reason: None,
            created_at: "2026-10-27T10:00:00Z".into(),
        })
    }
}

fn open(
    github: Arc<FakeGitHub>,
    cx: &mut TestAppContext,
) -> (Entity<Feedback>, &mut VisualTestContext, tempfile::TempDir) {
    cx.update(|cx| {
        cx.set_global(Theme::new(ThemePreference::Light, WindowAppearance::Light));
        crate::i18n::I18n::init(crate::i18n::Locale::PtBr, cx);
        crate::ui::text_input::bind_keys(cx);
        crate::ui::text_area::bind_keys(cx);
    });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("feedback.json"));
    let (feedback, cx) =
        cx.add_window_view(move |window, cx| Feedback::new(github, store, window, cx));
    cx.run_until_parked();
    (feedback, cx, dir)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

/// Types the title (it has the focus when the dialog opens), then the
/// description.
fn write(feedback: &Entity<Feedback>, cx: &mut VisualTestContext, title: &str, body: &str) {
    cx.simulate_input(title);
    cx.update(|window, cx| {
        let area = feedback.read(cx).description.clone();
        let focus = gpui::Focusable::focus_handle(area.read(cx), cx);
        window.focus(&focus, cx);
    });
    cx.simulate_input(body);
    cx.run_until_parked();
}

#[gpui::test]
fn a_report_is_reviewed_published_and_leads_the_history(cx: &mut TestAppContext) {
    let github = Arc::new(FakeGitHub::default());
    let (feedback, cx, dir) = open(github.clone(), cx);
    assert_eq!(
        cx.update(|_, cx| feedback.read(cx).login()),
        Some("ana".into())
    );
    write(&feedback, cx, "Sidebar fails", "It does not open.");
    assert!(cx.update(|_, cx| feedback.read(cx).can_review()));
    // "Revisar feedback" shows the issue as it will read; "Publicar issue" sends it.
    click(cx, "feedback-submit");
    assert!(cx.debug_bounds("feedback-review").is_some());
    click(cx, "feedback-submit");
    let sent = github.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].login, "ana");
    assert_eq!(sent[0].title, "Sidebar fails");
    assert!(sent[0].include_diagnostics);
    cx.update(|_, cx| {
        let feedback = feedback.read(cx);
        assert_eq!(feedback.tab, Tab::History);
        assert_eq!(
            feedback.submitted.as_ref().map(|issue| issue.number),
            Some(42)
        );
        // A fresh draft for the next report.
        assert!(feedback.draft.title.is_empty());
        assert_ne!(feedback.draft.request_id, sent[0].request_id);
    });
    assert!(cx.debug_bounds("feedback-sent").is_some());
    assert!(cx.debug_bounds("feedback-issue-0").is_some());
    // The page GitHub answered is kept for next time.
    let kept = Store::new(dir.path().join("feedback.json")).history("ana", 1);
    assert_eq!(kept.unwrap().issues[0].number, 3);
}

#[gpui::test]
fn an_unconfirmed_answer_keeps_the_same_request(cx: &mut TestAppContext) {
    let github = Arc::new(FakeGitHub {
        failures: Mutex::new(vec!["network down".into()]),
        ..Default::default()
    });
    let (feedback, cx, _dir) = open(github.clone(), cx);
    write(&feedback, cx, "Crash", "On start.");
    click(cx, "feedback-submit");
    click(cx, "feedback-submit");
    cx.update(|_, cx| {
        let feedback = feedback.read(cx);
        // A lost answer may hide a published issue: "uncertain", still pending.
        assert_eq!(feedback.error, Some("uncertain"));
        assert!(feedback.draft.pending.is_some());
    });
    assert!(cx.debug_bounds("feedback-error").is_some());
    // "Verificar envio" sends the same request id again.
    click(cx, "feedback-submit");
    let sent = github.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].request_id, sent[1].request_id);
    assert_eq!(cx.update(|_, cx| feedback.read(cx).tab), Tab::History);
}

#[gpui::test]
fn without_an_account_it_asks_to_connect(cx: &mut TestAppContext) {
    let github = Arc::new(FakeGitHub {
        no_account: true,
        ..Default::default()
    });
    let (feedback, cx, _dir) = open(github, cx);
    assert!(cx.debug_bounds("feedback-connect-hint").is_some());
    write(&feedback, cx, "Title", "Body");
    assert!(!cx.update(|_, cx| feedback.read(cx).can_review()));
    // The kinds switch the description's placeholder; "Melhoria" has no
    // reproduction details.
    click(cx, "feedback-category-improvement");
    assert!(cx.debug_bounds("feedback-reproduction").is_none());
    click(cx, "feedback-category-bug");
    assert!(cx.debug_bounds("feedback-reproduction").is_some());
}
