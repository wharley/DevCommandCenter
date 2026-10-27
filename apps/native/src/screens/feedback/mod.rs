//! "Feedback DCC" (`FeedbackDialog`): a report (bug, improvement or other)
//! published as an issue on the DCC repository under the person's GitHub
//! account, reviewed before it goes, its draft kept on this device; and
//! "Meus feedbacks", the issues that account opened there, page by page.

mod form;
mod history;
#[cfg(test)]
mod tests;
mod view;

use std::sync::Arc;

use dcc_infra::feedback::{FeedbackContext, FeedbackIssue, FeedbackPage};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, Window};

use crate::data::feedback::{Category, Draft, SharedBackend, Store, error_key};
use crate::ui::text_area::{TextArea, TextAreaEvent};
use crate::ui::text_input::{TextInput, TextInputEvent};

pub enum FeedbackEvent {
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    New,
    History,
}

/// The account query (`dccFeedbackContext`).
#[derive(Default)]
struct Account {
    context: Option<FeedbackContext>,
    fetching: bool,
    failed: bool,
}

/// The history query of one page (`dccFeedbackHistory`).
#[derive(Default)]
struct History {
    data: Option<FeedbackPage>,
    fetching: bool,
    failed: bool,
}

/// One text field and what it holds in the draft.
#[derive(Clone, Copy)]
enum Field {
    Description,
    Steps,
    Expected,
}

pub struct Feedback {
    backend: SharedBackend,
    store: Arc<Store>,
    tab: Tab,
    draft: Draft,
    reviewing: bool,
    sending: bool,
    /// A `feedback.errors.*` key.
    error: Option<&'static str>,
    /// The draft could be kept on this device.
    saved: bool,
    submitted: Option<FeedbackIssue>,
    page: u32,
    account: Account,
    history: History,
    /// "Adicionar detalhes de reprodução" open (`<details>`).
    details_open: bool,
    title: Entity<TextInput>,
    description: Entity<TextArea>,
    steps: Entity<TextArea>,
    expected: Entity<TextArea>,
    _fields: Vec<Subscription>,
    _account: Option<Task<()>>,
    _history: Option<Task<()>>,
    _send: Option<Task<()>>,
}

impl EventEmitter<FeedbackEvent> for Feedback {}

/// `maxLength` on the field.
fn limit(text: &str, max: usize) -> Option<String> {
    (text.chars().count() > max).then(|| text.chars().take(max).collect())
}

impl Feedback {
    pub fn new(
        backend: SharedBackend,
        store: Store,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = store.draft();
        let subject = crate::i18n::Translate::t(&**cx, "feedback.subjectPlaceholder");
        let title = cx.new(|cx| {
            let mut input = TextInput::new(window, cx)
                .with_placeholder(subject)
                .with_max_len(200);
            input.set_text(draft.title.clone(), cx);
            input
        });
        let placeholder = crate::i18n::Translate::t(
            &**cx,
            &format!("feedback.descriptionPlaceholder.{}", draft.category.key()),
        );
        let area = |text: &str,
                    placeholder: Option<gpui::SharedString>,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            let text = text.to_string();
            cx.new(|cx| {
                let mut area = TextArea::new(window, cx);
                if let Some(placeholder) = placeholder {
                    area = area.with_placeholder(placeholder);
                }
                area.set_text(&text, cx);
                area
            })
        };
        let description = area(&draft.description, Some(placeholder), window, cx);
        let steps = area(&draft.steps, None, window, cx);
        let expected = area(&draft.expected, None, window, cx);
        let mut fields = vec![
            cx.subscribe(&title, |feedback, input, event, cx| match event {
                TextInputEvent::Changed => {
                    let text = input.read(cx).text().to_string();
                    feedback.edit(|draft| draft.title = text, cx);
                }
                // Enter submits the form (implicit submission).
                TextInputEvent::Submitted => feedback.submit_form(cx),
                TextInputEvent::Cancelled => cx.emit(FeedbackEvent::Closed),
                TextInputEvent::Blurred => {}
            }),
        ];
        for (entity, field) in [
            (&description, Field::Description),
            (&steps, Field::Steps),
            (&expected, Field::Expected),
        ] {
            fields.push(cx.subscribe(entity, move |feedback, area, event, cx| {
                feedback.area_event(&area, field, event, cx)
            }));
        }
        let mut feedback = Self {
            backend,
            store: Arc::new(store),
            tab: Tab::New,
            draft,
            reviewing: false,
            sending: false,
            error: None,
            saved: true,
            submitted: None,
            page: 1,
            account: Account::default(),
            history: History::default(),
            details_open: false,
            title,
            description,
            steps,
            expected,
            _fields: fields,
            _account: None,
            _history: None,
            _send: None,
        };
        feedback.load_account(cx);
        feedback
    }

    pub fn title_input(&self) -> Entity<TextInput> {
        self.title.clone()
    }

    fn area_event(
        &mut self,
        area: &Entity<TextArea>,
        field: Field,
        event: &TextAreaEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextAreaEvent::Changed => {
                let max = match field {
                    Field::Description => 5000,
                    Field::Steps | Field::Expected => 2000,
                };
                let mut text = area.read(cx).text().to_string();
                if let Some(cut) = limit(&text, max) {
                    area.update(cx, |area, cx| area.set_text(&cut, cx));
                    text = cut;
                }
                self.edit(
                    |draft| match field {
                        Field::Description => draft.description = text,
                        Field::Steps => draft.steps = text,
                        Field::Expected => draft.expected = text,
                    },
                    cx,
                );
            }
            // A text box: Enter is a new line.
            TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
            TextAreaEvent::Cancelled => cx.emit(FeedbackEvent::Closed),
            _ => {}
        }
    }

    fn login(&self) -> Option<String> {
        self.account
            .context
            .as_ref()
            .map(|context| context.login.clone())
    }

    fn load_account(&mut self, cx: &mut Context<Self>) {
        self.account.fetching = true;
        cx.notify();
        let backend = self.backend.clone();
        self._account = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { backend.context() })
                .await;
            let _ = this.update(cx, |feedback, cx| {
                let before = feedback.login();
                feedback.account.fetching = false;
                match read {
                    Ok(context) => {
                        feedback.account.failed = false;
                        feedback.account.context = Some(context);
                    }
                    Err(error) => {
                        eprintln!("[dcc-native] feedback account: {error}");
                        feedback.account.failed = true;
                        feedback.account.context = None;
                    }
                }
                // Another account: back to its first page, nothing in review.
                if feedback.login() != before {
                    feedback.page = 1;
                    feedback.reviewing = false;
                    feedback.submitted = None;
                    if feedback.tab == Tab::History {
                        feedback.load_history(cx);
                    }
                }
                cx.notify();
            });
        }));
    }

    /// The page shows what was kept for it at once, then reads GitHub.
    fn load_history(&mut self, cx: &mut Context<Self>) {
        let Some(login) = self.login() else {
            return;
        };
        let page = self.page;
        self.history = History {
            data: self.store.history(&login, page),
            fetching: true,
            failed: false,
        };
        cx.notify();
        let (backend, store) = (self.backend.clone(), self.store.clone());
        self._history = Some(cx.spawn(async move |this, cx| {
            let wanted = login.clone();
            let read = cx
                .background_executor()
                .spawn(async move { backend.list(&wanted, page) })
                .await;
            let _ = this.update(cx, |feedback, cx| {
                if feedback.page != page || feedback.login().as_deref() != Some(login.as_str()) {
                    return;
                }
                feedback.history.fetching = false;
                match read {
                    Ok(result) => {
                        store.save_history(&login, page, &result);
                        feedback.history.data = Some(result);
                    }
                    Err(error) => {
                        eprintln!("[dcc-native] feedback history: {error}");
                        feedback.history.failed = true;
                    }
                }
                cx.notify();
            });
        }));
    }

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        self.tab = tab;
        if tab == Tab::History {
            self.load_history(cx);
        }
        cx.notify();
    }

    fn set_page(&mut self, page: u32, cx: &mut Context<Self>) {
        self.page = page.max(1);
        self.load_history(cx);
    }

    /// `edit`: every change is kept, clears the error and leaves review.
    fn edit(&mut self, change: impl FnOnce(&mut Draft), cx: &mut Context<Self>) {
        change(&mut self.draft);
        self.saved = self.store.save_draft(&self.draft);
        self.error = None;
        self.reviewing = false;
        cx.notify();
    }

    fn set_category(&mut self, category: Category, cx: &mut Context<Self>) {
        let placeholder = crate::i18n::Translate::t(
            &**cx,
            &format!("feedback.descriptionPlaceholder.{}", category.key()),
        );
        self.description
            .update(cx, |area, cx| area.set_placeholder(placeholder, cx));
        self.edit(|draft| draft.category = category, cx);
    }

    /// `startNew`
    fn start_new(&mut self, cx: &mut Context<Self>) {
        self.draft = Draft::empty();
        self.saved = self.store.save_draft(&self.draft);
        for area in [&self.description, &self.steps, &self.expected] {
            area.update(cx, |area, cx| area.set_text("", cx));
        }
        self.title.update(cx, |input, cx| input.set_text("", cx));
        // Clearing the fields edited the fresh draft: it stays fresh.
        self.reviewing = false;
        self.error = None;
        self.tab = Tab::New;
        cx.notify();
    }

    pub(super) fn can_review(&self) -> bool {
        self.login().is_some()
            && !self.account.failed
            && !self.draft.title.trim().is_empty()
            && !self.draft.description.trim().is_empty()
    }

    pub(super) fn show_review(&self) -> bool {
        self.reviewing || self.draft.pending.is_some()
    }

    /// The form's submit: publish from the review, else open the review.
    fn submit_form(&mut self, cx: &mut Context<Self>) {
        if self.show_review() {
            self.submit(cx);
        } else if self.can_review() {
            self.reviewing = true;
            cx.notify();
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(login) = self.login() else {
            return;
        };
        if self.sending
            || !self.can_review()
            || self
                .draft
                .pending
                .as_ref()
                .is_some_and(|pending| pending.login != login)
        {
            return;
        }
        self.sending = true;
        self.error = None;
        let request = self.draft.request(&login);
        self.draft.pending = Some(request.clone());
        self.saved = self.store.save_draft(&self.draft);
        cx.notify();
        let (backend, store) = (self.backend.clone(), self.store.clone());
        self._send = Some(cx.spawn(async move |this, cx| {
            let input = request.input();
            let sent = cx
                .background_executor()
                .spawn(async move { backend.create(input) })
                .await;
            let _ = this.update(cx, |feedback, cx| {
                feedback.sending = false;
                match sent {
                    Ok(issue) => {
                        // The new issue leads the first page right away.
                        let previous = store.history(&request.login, 1);
                        let mut issues = vec![issue.clone()];
                        issues.extend(
                            previous
                                .as_ref()
                                .map(|page| page.issues.clone())
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|item| item.number != issue.number),
                        );
                        issues.truncate(30);
                        let updated = FeedbackPage {
                            issues,
                            has_next: previous.is_some_and(|page| page.has_next),
                        };
                        store.save_history(&request.login, 1, &updated);
                        feedback.start_new(cx);
                        feedback.submitted = Some(issue);
                        feedback.page = 1;
                        feedback.tab = Tab::History;
                        feedback.load_history(cx);
                    }
                    Err(error) => {
                        let mapped = error_key(&error);
                        // A lost answer may hide a published issue: keep the request.
                        let reason = if mapped == "failed" {
                            "uncertain"
                        } else {
                            mapped
                        };
                        feedback.error = Some(reason);
                        if reason != "uncertain" && reason != "accountChanged" {
                            feedback.draft.pending = None;
                            feedback.saved = feedback.store.save_draft(&feedback.draft);
                        }
                        if reason == "accountChanged" {
                            feedback.reviewing = false;
                            feedback.load_account(cx);
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_issue(&self, number: Option<u64>, cx: &mut Context<Self>) {
        let login = self.login();
        cx.open_url(&crate::data::feedback::issue_url(number, login.as_deref()));
    }
}
