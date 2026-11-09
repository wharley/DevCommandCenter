//! Answering the agent from the conversation: the questions it asked
//! (`UserInputCard`) and the permissions it waits on (`ApprovalCard` in the
//! pending panel). Each answer goes to the desktop, which holds the agent
//! (`data::desktop_api`); the card stays live until the history shows it
//! resolved.

use std::cell::RefCell;
use std::collections::HashMap;

use dcc_core::ports::provider::ProviderUserInputAnswer;
use gpui::{AppContext, Context, Entity, SharedString, Window};

use super::ConversationView;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::Translate;
use crate::ui::{TextInput, Toaster};

/// What a request is waiting on while its answer travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sending {
    Allow,
    Deny,
    Answers,
}

#[derive(Default)]
pub(crate) struct Requests {
    /// One field per question, by `request id + question`.
    inputs: RefCell<HashMap<(String, String), Entity<TextInput>>>,
    pub(super) sending: HashMap<String, Sending>,
    /// "Recusar com motivo", while its field is open (`approval_reason`).
    pub(super) reason: Option<(String, Entity<TextInput>)>,
    /// Where answers go instead of the desktop's own endpoint (tests).
    pub endpoint: Option<DesktopApi>,
}

impl Requests {
    pub fn clear(&mut self) {
        self.inputs.borrow_mut().clear();
        self.sending.clear();
        self.reason = None;
    }
}

impl ConversationView {
    pub(crate) fn sending(&self, request_id: &str) -> Option<Sending> {
        self.requests.sending.get(request_id).copied()
    }

    /// The answer field of one question, made on first use with what was
    /// already answered.
    pub(crate) fn answer_input(
        &self,
        request_id: &str,
        question: &str,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let key = (request_id.to_string(), question.to_string());
        if let Some(input) = self.requests.inputs.borrow().get(&key) {
            return input.clone();
        }
        let placeholder = cx.t("conversation.userInput.placeholder");
        let initial = initial.to_string();
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx).with_placeholder(placeholder);
            input.set_text(initial, cx);
            input
        });
        // Typing enables "Enviar resposta"; redraw on every change.
        cx.observe(&input, |_, _, cx| cx.notify()).detach();
        self.requests.inputs.borrow_mut().insert(key, input.clone());
        input
    }

    pub(crate) fn answer_text(&self, request_id: &str, question: &str, cx: &gpui::App) -> String {
        self.requests
            .inputs
            .borrow()
            .get(&(request_id.to_string(), question.to_string()))
            .map(|input| input.read(cx).text().trim().to_string())
            .unwrap_or_default()
    }

    /// An option chip fills its question's field, like the desktop's draft.
    pub(crate) fn pick_option(
        &mut self,
        request_id: &str,
        question: &str,
        label: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self
            .requests
            .inputs
            .borrow()
            .get(&(request_id.to_string(), question.to_string()))
        {
            input.update(cx, |input, cx| input.set_text(label, cx));
        }
        cx.notify();
    }

    pub(crate) fn respond_permission(
        &mut self,
        request_id: String,
        allow: bool,
        cx: &mut Context<Self>,
    ) {
        let session = self.session_id();
        let request = request_id.clone();
        self.send(
            request_id,
            if allow { Sending::Allow } else { Sending::Deny },
            move |api| api.respond_permission(&session, &request, allow),
            cx.t("conversation.permission.submitError"),
            cx,
        );
    }

    pub(crate) fn send_answers(
        &mut self,
        request_id: String,
        questions: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let answers: Vec<ProviderUserInputAnswer> = questions
            .into_iter()
            .map(|question| ProviderUserInputAnswer {
                answer: self.answer_text(&request_id, &question, cx),
                question,
            })
            .collect();
        if answers.iter().any(|answer| answer.answer.is_empty()) {
            return;
        }
        let session = self.session_id();
        let request = request_id.clone();
        self.send(
            request_id,
            Sending::Answers,
            move |api| api.respond_user_input(&session, &request, &answers),
            cx.t("conversation.userInput.submitError"),
            cx,
        );
    }

    fn session_id(&self) -> String {
        self.data
            .as_ref()
            .and_then(|data| data.session_id.clone())
            .unwrap_or_default()
    }

    fn send(
        &mut self,
        request_id: String,
        kind: Sending,
        job: impl FnOnce(&DesktopApi) -> Result<(), ApiError> + Send + 'static,
        failure: SharedString,
        cx: &mut Context<Self>,
    ) {
        if self.requests.sending.contains_key(&request_id) {
            return;
        }
        self.requests.sending.insert(request_id.clone(), kind);
        let endpoint = self.requests.endpoint.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = endpoint
                        .or_else(DesktopApi::load)
                        .ok_or(ApiError::DesktopClosed)?;
                    job(&api)
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.requests.sending.remove(&request_id);
                match result {
                    Ok(()) => view.refresh_now(cx),
                    Err(ApiError::AlreadyAnswered) => {
                        Toaster::info(cx.t("native.alreadyAnswered"), cx);
                        view.refresh_now(cx);
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error_with(failure, error.to_string(), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
