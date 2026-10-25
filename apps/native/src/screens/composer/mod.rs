//! Screen 3: the composer (`WorkspaceComposer`) under the conversation. The
//! prompt goes to the desktop, which runs the agent (`data::desktop_api`):
//! Enter sends, the stop button interrupts, and a draft is kept per
//! conversation while you move between tasks. Files go in as chips: the +
//! menu, a pasted image, a drop, or `@arquivo`.

mod annotation;
mod attachments;
mod context_rail;
mod controls;
mod drafts;
mod first_send;
mod handoff;
mod mentions;
mod objective;
mod picker;
mod popups;
mod prompt;
mod queue;
mod settings;
pub(crate) use settings::load_settings;
mod strips;
mod surface;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui::{App, Context, Entity, EventEmitter, Focusable, Subscription, Window, prelude::*};

use crate::data::desktop_api::{ApiError, DesktopApi};
use crate::i18n::Translate;
use crate::ui::{TextArea, TextAreaEvent, Toaster};

/// Past this the prompt scrolls inside the field (`max-h-[240px]`).
const INPUT_MAX_HEIGHT: f32 = 240.0;

/// What the conversation tells the composer about the open session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposerSession {
    /// The task (workspace) the conversation belongs to.
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub running: bool,
    pub aborted: bool,
    pub workspace_path: Option<String>,
    pub workspace_name: Option<String>,
    pub branch: Option<String>,
    pub usage_limit: Option<dcc_core::domain::usage_limit::SessionUsageLimit>,
    pub goal: Option<dcc_core::domain::goal::SessionGoal>,
    /// The task's project, for the per-project model pick.
    pub project_id: Option<String>,
    /// No conversation yet: the first prompt starts one.
    pub start: Option<crate::data::conversation::ThreadStart>,
    /// How another conversation of the task starts ("Nova sessão").
    pub new_thread: Option<crate::data::conversation::ThreadStart>,
    /// Follow-ups waiting for the running turn (`project_turn_queue`).
    pub queue: std::sync::Arc<Vec<dcc_core::domain::session::QueuedTurn>>,
    /// The DCC's own objective (the toolbar's "Objetivo").
    pub objective: Option<dcc_core::domain::objective::SessionObjective>,
    pub auto_resume: bool,
    pub context: Option<crate::data::conversation::ExecutionContext>,
}

impl ComposerSession {
    /// The field takes a prompt: a conversation, or a task about to start one.
    pub fn usable(&self) -> bool {
        self.session_id.is_some() || self.start.is_some()
    }

    /// Where the field's draft is kept: the conversation, else the task.
    pub fn draft_key(&self) -> Option<String> {
        self.session_id.clone().or_else(|| {
            self.start
                .as_ref()
                .map(|start| format!("task:{}", start.workspace_id))
        })
    }
}

/// Something the conversation should do after the composer acted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposerEvent {
    /// A turn was sent or stopped: read the history again now.
    Refresh,
    /// A conversation the composer just started elsewhere in the task.
    OpenSession(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Busy {
    Sending,
    Stopping,
    Queueing,
    Steering,
}

pub struct Composer {
    pub(crate) input: Entity<TextArea>,
    pub(crate) session: ComposerSession,
    /// Drafts by conversation, also kept on disk.
    drafts: HashMap<String, String>,
    pub(crate) busy: Option<Busy>,
    /// Where the turns go instead of the desktop's own endpoint (tests).
    pub(crate) endpoint: Option<DesktopApi>,
    /// The `@arquivo` popup, while open.
    pub(crate) mention: Option<mentions::MentionMenu>,
    pub(crate) files: Option<mentions::FileList>,
    files_loading: bool,
    /// The desktop's providers, as its composer sees them.
    pub(crate) catalog: Option<std::sync::Arc<Vec<dcc_core::domain::provider::ProviderDescriptor>>>,
    pub(crate) catalog_at: Option<std::time::Instant>,
    catalog_loading: bool,
    pub(crate) settings: crate::data::composer_settings::ComposerSettings,
    pub(crate) approval_menu_open: bool,
    /// "Ativar acesso completo?" waits for an answer.
    pub(crate) confirm_full_access: bool,
    /// The conversation so far, for a provider handoff.
    pub(crate) history: std::sync::Arc<Vec<dcc_core::domain::timeline::TimelineMessage>>,
    pub(crate) picker: picker::PickerState,
    /// A strip's action (resume, goal, auto-resume) on its way.
    pub(crate) strip_busy: Option<strips::StripAction>,
    /// The execution-mode explainer under the composer.
    pub(crate) mode_open: bool,
    pub(crate) objective: objective::ObjectiveState,
    /// A queue tray action on its way.
    pub(crate) queue_busy: Option<queue::QueueAction>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Composer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = cx.t("composer.placeholder.default");
        let input = cx.new(|cx| {
            TextArea::new(window, cx)
                .with_placeholder(placeholder)
                .with_max_height(INPUT_MAX_HEIGHT)
        });
        let input_events = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &TextAreaEvent, window, cx| match event {
                TextAreaEvent::Submitted => this.send(window, cx),
                TextAreaEvent::Changed => this.remember_draft(cx),
                TextAreaEvent::MenuUp => this.move_mention(-1, cx),
                TextAreaEvent::MenuDown => this.move_mention(1, cx),
                TextAreaEvent::MenuAccept => this.accept_mention(None, cx),
                TextAreaEvent::Cancelled => this.close_mention(cx),
                TextAreaEvent::PastedNonText => this.paste_image(cx),
                TextAreaEvent::Blurred => {}
            },
        );
        // Typing or moving the caret may open, filter or close the popup.
        let input_changes = cx.observe(&input, |this, _, cx| this.update_mention(cx));
        Self {
            input,
            session: ComposerSession::default(),
            drafts: drafts::load(),
            busy: None,
            endpoint: None,
            mention: None,
            files: None,
            files_loading: false,
            // The built-in descriptors until the desktop's live list arrives.
            catalog: Some(std::sync::Arc::new(
                dcc_providers::offline_catalog().providers,
            )),
            catalog_at: None,
            catalog_loading: false,
            settings: settings::load_settings(),
            approval_menu_open: false,
            confirm_full_access: false,
            history: Default::default(),
            picker: picker::PickerState::new(window, cx),
            strip_busy: None,
            mode_open: false,
            objective: objective::ObjectiveState::new(window, cx),
            queue_busy: None,
            _input_events: input_events,
            _input_changes: input_changes,
        }
    }

    /// Follows the conversation: switching sessions swaps the draft.
    pub fn set_session(&mut self, session: ComposerSession, cx: &mut Context<Self>) {
        if session == self.session {
            return;
        }
        let switched = session.draft_key() != self.session.draft_key();
        self.session = session;
        if switched {
            let draft = self
                .session
                .draft_key()
                .and_then(|id| self.drafts.get(&id))
                .cloned()
                .unwrap_or_default();
            let parts = prompt::parts_of(&draft);
            self.input
                .update(cx, |input, cx| input.set_parts(parts, cx));
            self.mention = None;
        }
        self.sync_objective(switched, cx);
        let disabled = !self.session.usable();
        let placeholder = if self.plan_mode() {
            cx.t("composer.placeholder.plan")
        } else {
            cx.t("composer.placeholder.default")
        };
        self.input.update(cx, |input, cx| {
            input.set_disabled(disabled, cx);
            input.set_placeholder(placeholder, cx);
        });
        if !disabled {
            self.refresh_catalog(cx);
        }
        cx.notify();
    }

    pub fn set_history(
        &mut self,
        history: std::sync::Arc<Vec<dcc_core::domain::timeline::TimelineMessage>>,
    ) {
        self.history = history;
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// "Mencionar no chat" / a file dropped from the code column: `@path`
    /// at the caret, then the field takes the focus.
    pub fn mention_file(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session.usable() {
            return;
        }
        let chip = crate::ui::text_area::Chip::File(path.to_string());
        self.input.update(cx, |input, cx| {
            input.insert_chip(chip, None, cx);
            input.insert(" ", cx);
        });
        self.focus(window, cx);
    }

    /// `appendComposerText`: a request from elsewhere (a failed delivery, PR
    /// conversations) after what is there, a blank line between; false when
    /// the composer can't take it.
    pub fn append_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.session.usable() || text.trim().is_empty() {
            return false;
        }
        self.input.update(cx, |input, cx| {
            let lead = if input.text().trim().is_empty() {
                ""
            } else {
                "\n\n"
            };
            input.append_text(&format!("{lead}{text}"), cx);
        });
        self.focus(window, cx);
        true
    }

    /// Puts text in the field (the continuation prompt), replacing the draft.
    pub fn replace_draft(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let parts = prompt::parts_of(text);
        self.input
            .update(cx, |input, cx| input.set_parts(parts, cx));
        self.focus(window, cx);
    }

    fn remember_draft(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.draft_key() else {
            return;
        };
        let text = self.prompt(cx);
        let changed = if text.trim().is_empty() {
            self.drafts.remove(&session).is_some()
        } else {
            self.drafts.insert(session, text.clone()) != Some(text)
        };
        if changed {
            drafts::save_later(self.drafts.clone(), cx);
        }
        cx.notify();
    }

    /// What would be sent now: the text with its chips as `@path`.
    pub(crate) fn prompt(&self, cx: &App) -> String {
        let input = self.input.read(cx);
        prompt::serialize(input.text(), &input.chips())
    }

    /// Something to send, and nobody waiting on the desktop. While a turn
    /// runs it goes to the queue (`canQueueActiveTurn`).
    pub(crate) fn can_send(&self, cx: &App) -> bool {
        (self.session.session_id.is_some() || self.new_task_pick().is_some())
            && self.busy.is_none()
            && !self.prompt(cx).trim().is_empty()
    }

    /// Enter or the send button; Enter while the agent works queues.
    pub(crate) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send(cx) {
            return;
        }
        if self.session.running {
            self.follow_up(queue::FollowUp::Queue, cx);
            return;
        }
        let prompt = self.prompt(cx);
        self.submit(prompt, None, true, window, cx);
    }

    /// "Tentar de novo": the interrupted turn's prompt again, linked to it.
    pub fn retry(
        &mut self,
        prompt: String,
        turn_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.session_id.is_none() || self.busy.is_some() {
            return;
        }
        self.submit(prompt, Some(turn_id), false, window, cx);
    }

    /// "Continuar": reopens the session when it was stopped and leaves the
    /// continuation prompt in the field for the person to send.
    pub fn continue_interrupted(
        &mut self,
        original: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preamble = cx.t("conversation.message.continuePrompt").to_string();
        let label = cx.t("conversation.message.originalPrompt").to_string();
        let text = continuation_prompt(original.as_deref(), &preamble, &label);
        if self.session.aborted
            && let Some(session) = self.session.session_id.clone()
        {
            let endpoint = self.endpoint.clone();
            let failure = cx.t("conversation.message.resumeFailed");
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api(endpoint)?.resume(&session) })
                    .await;
                let _ = this.update(cx, |_, cx| {
                    if let Err(error) = result {
                        report(error, failure, cx);
                    }
                    cx.emit(ComposerEvent::Refresh);
                });
            })
            .detach();
        }
        self.replace_draft(&text, window, cx);
    }

    /// A turn the composer writes itself (the goal's resume or clear):
    /// sent as is, never in plan mode (`sendGoalPrompt`).
    pub(crate) fn send_text(&mut self, prompt: &str, cx: &mut Context<Self>) {
        if self.session.session_id.is_none() || self.busy.is_some() {
            return;
        }
        let options = crate::data::desktop_api::TurnOptions {
            plan_mode: false,
            ..self.turn_options(prompt)
        };
        self.dispatch(prompt.to_string(), None, options, false, cx);
    }

    fn submit(
        &mut self,
        prompt: String,
        retry_of: Option<String>,
        from_field: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = if retry_of.is_some() {
            self.retry_options(&prompt)
        } else {
            self.turn_options(&prompt)
        };
        self.dispatch(prompt, retry_of, options, from_field, cx);
    }

    fn dispatch(
        &mut self,
        prompt: String,
        retry_of: Option<String>,
        options: crate::data::desktop_api::TurnOptions,
        from_field: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session.session_id.clone() else {
            if retry_of.is_none() {
                self.first_send(prompt, options, from_field, cx);
            }
            return;
        };
        let resume_first = self.session.aborted && retry_of.is_some();
        let handoff = options
            .provider_id
            .clone()
            .map(|destination| handoff::HandoffRequest {
                source_provider: self.session.provider_id.clone(),
                destination_provider: destination,
                workspace_name: self.session.workspace_name.clone(),
                workspace_path: self.session.workspace_path.clone(),
                branch: self.session.branch.clone(),
                history: self.history.clone(),
                prompt: prompt.clone(),
                db_path: crate::data::loader::database_path(),
            });
        let endpoint = self.endpoint.clone();
        let failure = if retry_of.is_some() {
            cx.t("conversation.message.retryFailed")
        } else {
            cx.t("native.sendFailed")
        };
        self.busy = Some(Busy::Sending);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let api = api(endpoint)?;
                    let mut options = options;
                    if let Some(request) = handoff {
                        options.tool_instructions =
                            dcc_core::domain::turn_instructions::merge_tool_instructions(
                                options.tool_instructions.as_deref(),
                                handoff::build(&request).as_deref(),
                            );
                    }
                    if resume_first {
                        api.resume(&session)?;
                    }
                    api.send_turn(&session, &prompt, retry_of.as_deref(), &options)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(()) => {
                        if from_field {
                            this.clear_field(cx);
                        }
                        // The turn is on its way: the stop button shows until
                        // the history says otherwise.
                        this.session.running = true;
                    }
                    Err(error) => report(error, failure, cx),
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_field(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        if let Some(session) = &self.session.draft_key()
            && self.drafts.remove(session).is_some()
        {
            drafts::save_later(self.drafts.clone(), cx);
        }
    }

    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.session_id.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let endpoint = self.endpoint.clone();
        let failure = cx.t("native.stopFailed");
        self.busy = Some(Busy::Stopping);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api(endpoint)?.abort(&session) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                if let Err(error) = result {
                    report(error, failure, cx);
                }
                cx.emit(ComposerEvent::Refresh);
                cx.notify();
            });
        })
        .detach();
    }
}

fn api(endpoint: Option<DesktopApi>) -> Result<DesktopApi, ApiError> {
    endpoint
        .or_else(DesktopApi::load)
        .ok_or(ApiError::DesktopClosed)
}

fn report(error: ApiError, failure: gpui::SharedString, cx: &mut App) {
    match error {
        ApiError::DesktopClosed => Toaster::error(cx.t("native.desktopClosed"), cx),
        error => Toaster::error_with(failure, error.to_string(), cx),
    }
}

/// `buildSafeContinuationPrompt`
pub fn continuation_prompt(original: Option<&str>, preamble: &str, label: &str) -> String {
    match original
        .map(str::trim)
        .filter(|original| !original.is_empty())
    {
        Some(original) => format!("{}\n\n{}:\n{original}", preamble.trim(), label.trim()),
        None => preamble.trim().to_string(),
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        surface::render(self, window, cx)
    }
}
