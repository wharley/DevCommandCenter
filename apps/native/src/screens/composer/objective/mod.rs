//! The DCC's own task objective in the composer (`SessionObjectiveControl`):
//! the record comes from the database with the conversation; saving,
//! pausing, resuming, completing and clearing go through the desktop, which
//! re-sends the objective with every turn.

mod panel;

pub(super) use panel::objective_control;

use dcc_core::domain::objective::{
    ObjectivePauseReason, ObjectiveStatus, ObjectiveTransition, SessionObjective,
};
use dcc_core::domain::objective_form::ObjectiveForm;
use gpui::{AppContext, Context, Entity, Focusable, Subscription, Window};

use super::Composer;
use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::Translate;
use crate::ui::text_area::{TextArea, TextAreaEvent};
use crate::ui::{TextInput, TextInputEvent, Toaster};

/// What the panel is waiting on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectiveBusy {
    Save,
    Transition(ObjectiveTransition),
    Clear,
}

/// The panel's fields and state while the composer lives.
pub(crate) struct ObjectiveState {
    pub open: bool,
    pub busy: Option<ObjectiveBusy>,
    pub intent: Entity<TextArea>,
    pub done_when: Entity<TextArea>,
    pub max_failures: Entity<TextInput>,
    pub max_turns: Entity<TextInput>,
    /// The form as last filled from the record: anything else is the
    /// person's typing.
    filled_form: ObjectiveForm,
    /// The record generation the fields were filled from (`Some(None)`: no
    /// objective), so a refresh that changed nothing leaves them alone.
    filled: Option<Option<u64>>,
    /// The status last seen for this conversation, to tell an automatic pause.
    seen: Option<(String, ObjectiveStatus)>,
    _events: Vec<Subscription>,
}

impl ObjectiveState {
    pub fn new(window: &mut Window, cx: &mut Context<Composer>) -> Self {
        let intent_hint = cx.t("composer.objective.intentPlaceholder");
        let done_hint = cx.t("composer.objective.doneWhenPlaceholder");
        let unlimited = cx.t("composer.objective.unlimited");
        let intent = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(intent_hint)
                .with_max_height(180.0)
        });
        let done_when = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(done_hint)
                .with_max_height(160.0)
        });
        let max_failures = cx.new(|cx| TextInput::new(window, cx).with_max_len(5));
        let max_turns = cx.new(|cx| {
            TextInput::new(window, cx)
                .with_placeholder(unlimited)
                .with_max_len(5)
        });
        let area = |composer: &mut Composer,
                    area: &Entity<TextArea>,
                    event: &TextAreaEvent,
                    window: &mut Window,
                    cx: &mut Context<Composer>| match event {
            TextAreaEvent::Changed => cx.notify(),
            // Enter is a new line here; nothing in the form sends on Enter.
            TextAreaEvent::Submitted => area.update(cx, |area, cx| area.insert("\n", cx)),
            TextAreaEvent::Cancelled => composer.set_objective_open(false, window, cx),
            _ => {}
        };
        let field = |composer: &mut Composer,
                     _: &Entity<TextInput>,
                     event: &TextInputEvent,
                     window: &mut Window,
                     cx: &mut Context<Composer>| match event {
            TextInputEvent::Changed => cx.notify(),
            TextInputEvent::Cancelled => composer.set_objective_open(false, window, cx),
            _ => {}
        };
        let events = vec![
            cx.subscribe_in(&intent, window, area),
            cx.subscribe_in(&done_when, window, area),
            cx.subscribe_in(&max_failures, window, field),
            cx.subscribe_in(&max_turns, window, field),
        ];
        let mut state = Self {
            open: false,
            busy: None,
            intent,
            done_when,
            max_failures,
            max_turns,
            filled_form: ObjectiveForm::default(),
            filled: None,
            seen: None,
            _events: events,
        };
        state.fill(None, cx);
        state
    }

    /// The fields as the record has them (or empty, three failures).
    fn fill(&mut self, objective: Option<&SessionObjective>, cx: &mut Context<Composer>) {
        let form = ObjectiveForm::from_record(objective);
        self.intent
            .update(cx, |field, cx| field.set_text(&form.intent, cx));
        self.done_when
            .update(cx, |field, cx| field.set_text(&form.done_when, cx));
        self.max_failures.update(cx, |field, cx| {
            field.set_text(form.max_consecutive_failures.clone(), cx)
        });
        self.max_turns
            .update(cx, |field, cx| field.set_text(form.max_turns.clone(), cx));
        self.filled_form = form;
        self.filled = Some(objective.map(|objective| objective.generation));
    }

    /// The person changed something since the form was filled.
    pub fn dirty(&self, cx: &gpui::App) -> bool {
        self.form(cx) != self.filled_form
    }

    pub fn form(&self, cx: &gpui::App) -> ObjectiveForm {
        ObjectiveForm {
            intent: self.intent.read(cx).text().to_string(),
            done_when: self.done_when.read(cx).text().to_string(),
            max_consecutive_failures: self.max_failures.read(cx).text().to_string(),
            max_turns: self.max_turns.read(cx).text().to_string(),
        }
    }
}

impl Composer {
    /// Called with every conversation update: refills an untouched form,
    /// starts over on another conversation, and says so when the backend
    /// paused the objective by itself (a budget or the failure limit).
    pub(super) fn sync_objective(&mut self, switched: bool, cx: &mut Context<Self>) {
        let objective = self.session.objective.clone();
        if switched {
            self.objective.open = false;
            self.objective.busy = None;
            self.objective.seen = None;
            self.objective.fill(objective.as_ref(), cx);
        } else if !self.objective.dirty(cx)
            && self.objective.filled
                != Some(objective.as_ref().map(|objective| objective.generation))
        {
            self.objective.fill(objective.as_ref(), cx);
        }
        let session = self.session.session_id.clone();
        let status = objective.as_ref().map(|objective| objective.status);
        if let (Some(objective), Some((seen_session, seen_status))) =
            (objective.as_ref(), self.objective.seen.as_ref())
        {
            let automatic = objective
                .pause_reason
                .is_some_and(|reason| reason != ObjectivePauseReason::Manual);
            if session.as_deref() == Some(seen_session.as_str())
                && *seen_status == ObjectiveStatus::Active
                && objective.status == ObjectiveStatus::Paused
                && automatic
            {
                let reason = match objective.pause_reason {
                    Some(ObjectivePauseReason::TurnBudget) => "turn_budget",
                    _ => "consecutive_failures",
                };
                let description = cx.t(&format!("composer.objective.pauseReason.{reason}"));
                Toaster::warning_with(cx.t("composer.objective.autoPaused"), description, cx);
            }
        }
        self.objective.seen = session.zip(status);
    }

    pub(crate) fn set_objective_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.objective.busy.is_some() || self.objective.open == open {
            return;
        }
        self.objective.open = open;
        if open {
            let handle = self.objective.intent.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        } else {
            self.focus(window, cx);
        }
        cx.notify();
    }

    /// One request to the desktop; the answer becomes the record shown.
    fn run_objective(
        &mut self,
        busy: ObjectiveBusy,
        job: impl FnOnce(&DesktopApi) -> Result<Option<SessionObjective>, ApiError> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.objective.busy.is_some() {
            return;
        }
        self.objective.busy = Some(busy);
        let endpoint = self.endpoint.clone();
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
            let _ = this.update(cx, |this, cx| {
                this.objective.busy = None;
                match result {
                    Ok(next) => {
                        this.session.objective = next;
                        let objective = this.session.objective.clone();
                        this.objective.fill(objective.as_ref(), cx);
                        match busy {
                            ObjectiveBusy::Save => {
                                Toaster::success(cx.t("composer.objective.saved"), cx)
                            }
                            ObjectiveBusy::Clear => this.objective.open = false,
                            ObjectiveBusy::Transition(_) => {}
                        }
                        cx.emit(super::ComposerEvent::Refresh);
                    }
                    Err(ApiError::DesktopClosed) => {
                        Toaster::error(cx.t("native.desktopClosed"), cx)
                    }
                    Err(error) => Toaster::error_with(
                        cx.t("composer.objective.failed"),
                        error.to_string(),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Definir objetivo" / "Salvar": the form's own check first.
    pub(crate) fn save_objective(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        let draft = match self.objective.form(cx).to_draft() {
            Ok(draft) => draft,
            Err(error) => {
                let reason = cx.t(&format!("composer.objective.errors.{}", error.key()));
                Toaster::error_with(cx.t("composer.objective.failed"), reason, cx);
                return;
            }
        };
        let expected = self
            .session
            .objective
            .as_ref()
            .map(|objective| objective.generation);
        self.run_objective(
            ObjectiveBusy::Save,
            move |api| api.set_objective(&session, &draft, expected),
            cx,
        );
    }

    pub(crate) fn transition_objective(
        &mut self,
        transition: ObjectiveTransition,
        cx: &mut Context<Self>,
    ) {
        let (Some(session), Some(objective)) = (
            self.session.session_id.clone(),
            self.session.objective.as_ref(),
        ) else {
            return;
        };
        let generation = objective.generation;
        self.run_objective(
            ObjectiveBusy::Transition(transition),
            move |api| api.transition_objective(&session, transition, generation),
            cx,
        );
    }

    pub(crate) fn clear_objective(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        self.run_objective(
            ObjectiveBusy::Clear,
            move |api| api.clear_objective(&session).map(|()| None),
            cx,
        );
    }
}
