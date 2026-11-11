//! An agent's question asked in the team room itself (its ask-user tool,
//! the rule in `dcc_core::domain::room_question`): docked on the field above
//! the approvals (`room_question_card`), answered one decision at a time
//! (`room_question_steps`) and sent with the conversation's own
//! `respond-user-input`, so the person does not leave the room to answer.
//! The card leaves when the history shows it answered, here or elsewhere.

use std::collections::{HashMap, HashSet};

use gpui::{AppContext, Context, Entity, Window};

use super::room::TeamRoom;
use super::room_question_steps::{Pick, QuestionDraft};
use crate::data::agents::AgentRecord;
use crate::data::desktop_api::ApiError;
use crate::data::room_questions::PendingQuestion;
use crate::i18n::Translate;
use crate::ui::{TextInput, Toaster};

/// One question waiting in the room.
#[derive(Clone)]
pub(crate) struct RoomQuestion {
    pub agent: AgentRecord,
    pub session_id: String,
    pub request: PendingQuestion,
}

/// The room's answers in progress, by request.
#[derive(Default)]
pub(crate) struct RoomQuestions {
    drafts: HashMap<String, QuestionDraft>,
    /// "Outra coisa"'s field, by request and decision.
    others: HashMap<(String, usize), Entity<TextInput>>,
    pub(super) sending: HashSet<String>,
    /// Sent and accepted: hidden until the history says it is answered.
    sent: HashSet<String>,
}

impl RoomQuestions {
    /// The picks of `question` so far (none yet: a fresh draft).
    pub(super) fn draft(&self, question: &RoomQuestion) -> QuestionDraft {
        self.drafts
            .get(&question.request.request_id)
            .cloned()
            .unwrap_or_else(|| QuestionDraft::new(question.request.questions.len()))
    }

    pub(super) fn other_field(&self, request_id: &str, index: usize) -> Option<Entity<TextInput>> {
        self.others.get(&(request_id.to_string(), index)).cloned()
    }

    pub(super) fn other_text(&self, request_id: &str, index: usize, cx: &gpui::App) -> String {
        self.other_field(request_id, index)
            .map(|input| input.read(cx).text().to_string())
            .unwrap_or_default()
    }

    fn draft_mut(&mut self, question: &RoomQuestion) -> &mut QuestionDraft {
        self.drafts
            .entry(question.request.request_id.clone())
            .or_insert_with(|| QuestionDraft::new(question.request.questions.len()))
    }
}

impl TeamRoom {
    /// The questions the room's conversations wait on, oldest room first.
    pub(crate) fn questions(&self, agents: &[AgentRecord]) -> Vec<RoomQuestion> {
        self.feeds
            .iter()
            .filter(|feed| self.shows(&feed.agent_id))
            .filter_map(|feed| {
                let request = feed.pending_question()?;
                if self.questions.sent.contains(&request.request_id) {
                    return None;
                }
                let agent = agents.iter().find(|agent| agent.id == feed.agent_id)?;
                Some(RoomQuestion {
                    agent: agent.clone(),
                    session_id: feed.session_id.clone(),
                    request,
                })
            })
            .collect()
    }

    /// An option or "Outra coisa" picked; the latter opens its field.
    pub(super) fn pick_answer(
        &mut self,
        question: &RoomQuestion,
        index: usize,
        pick: Pick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other = pick == Pick::Other;
        self.questions.draft_mut(question).pick(index, pick);
        if other {
            let key = (question.request.request_id.clone(), index);
            let input = match self.questions.others.get(&key) {
                Some(input) => input.clone(),
                None => {
                    let input = cx.new(|cx| {
                        TextInput::new(window, cx)
                            .with_placeholder(cx.t("agents.room.question.otherPlaceholder"))
                    });
                    // Writing enables "Próxima"; redraw as it changes.
                    cx.observe(&input, |_, _, cx| cx.notify()).detach();
                    self.questions.others.insert(key, input.clone());
                    input
                }
            };
            let handle = gpui::Focusable::focus_handle(input.read(cx), cx);
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    /// "Próxima" / "Revisar" (`forward`) or "Voltar".
    pub(super) fn step_question(
        &mut self,
        question: &RoomQuestion,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        let draft = self.questions.draft_mut(question);
        if forward {
            draft.next();
        } else {
            draft.back();
        }
        cx.notify();
    }

    /// "Enviar": every decision's answer to the agent, off the UI thread.
    pub(super) fn send_question(&mut self, question: &RoomQuestion, cx: &mut Context<Self>) {
        let request = question.request.request_id.clone();
        let draft = self.questions.draft(question);
        let Some(answers) = draft.answers(&question.request.questions, |index| {
            self.questions.other_text(&request, index, cx)
        }) else {
            return;
        };
        let Some(api) = self.api.clone() else {
            Toaster::error(cx.t("native.desktopClosed"), cx);
            return;
        };
        if !self.questions.sending.insert(request.clone()) {
            return;
        }
        cx.notify();
        let session = question.session_id.clone();
        cx.spawn(async move |this, cx| {
            let request_id = request.clone();
            let sent = cx
                .background_executor()
                .spawn(async move { api.respond_user_input(&session, &request_id, &answers) })
                .await;
            let _ = this.update(cx, |room, cx| {
                room.questions.sending.remove(&request);
                match sent {
                    Ok(()) => {
                        room.questions.sent.insert(request);
                    }
                    Err(ApiError::AlreadyAnswered) => {
                        room.questions.sent.insert(request);
                        Toaster::info(cx.t("native.alreadyAnswered"), cx);
                    }
                    Err(error) => Toaster::error_with(
                        cx.t("agents.room.question.failed"),
                        error.to_string(),
                        cx,
                    ),
                }
                if !cfg!(test) {
                    room.sidebar
                        .update(cx, |sidebar, cx| sidebar.reload_now(cx));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
