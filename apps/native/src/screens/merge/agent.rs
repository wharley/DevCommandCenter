//! "Resolver arquivo com agente" (`resolveWithAgent` and the desktop's
//! `handleResolveConflictWithAgent`): a conversation of the task on the
//! composer's pick, one read-only turn (medium effort, approvals asked)
//! with the file's sides, followed every 250ms for five minutes at most,
//! cancellable; the answer's envelope checked and put in the editor for
//! review — nothing is saved. The editor stays read-only while it runs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dcc_core::domain::merge_conflict::agent::{
    AGENT_RESOLUTION_TOOL_INSTRUCTIONS, AgentResolutionInput, AgentTurnEnd, agent_aborted_message,
    agent_turn_end, build_agent_resolution_prompt, new_resolution_token, parse_agent_resolution,
    validate_agent_resolution,
};
use dcc_core::domain::session::SessionEventRecord;
use dcc_core::domain::timeline::{Role, project_session};
use gpui::Context;

use super::{Busy, MergeResolver, MergeResolverEvent, Pending, ResultView};
use crate::data::desktop_api::{ApiError, TurnOptions};
use crate::i18n::{Args, Locale, Translate};
use crate::ui::Toaster;

/// `AGENT_RESOLUTION_POLL_INTERVAL_MS`
const POLL: Duration = Duration::from_millis(250);
/// `AGENT_RESOLUTION_TIMEOUT_MS`
const TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Analyzing,
    Cancelling,
    Completed,
    Failed,
}

/// `AgentRunState`
pub(crate) struct AgentRun {
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub events: Vec<SessionEventRecord>,
    pub status: AgentStatus,
    pub expanded: bool,
    pub error: Option<String>,
    /// Fences this run's request and answer.
    pub token: String,
    cancel: Arc<AtomicBool>,
}

/// `AppliedAgentSuggestion`
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppliedSuggestion {
    pub path: String,
    pub explanation: String,
    pub session_id: String,
}

/// How the run ended, or what stopped it.
enum Outcome {
    Answer(String),
    Failed(String),
}

impl MergeResolver {
    /// Starts the run on the open file's result as it is now.
    pub(crate) fn resolve_with_agent(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let Some(pick) = self.request.agent.clone() else {
            Toaster::error(
                "Selecione um workspace local e um provedor disponível para usar o agente.",
                cx,
            );
            return;
        };
        let ((_, current_ref), (_, incoming_ref)) = self.labels(cx);
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let token = new_resolution_token();
        let entry = open.entry.clone();
        let source = open.buffer.clone();
        let prompt = build_agent_resolution_prompt(
            &AgentResolutionInput {
                path: &entry.path,
                kind: entry.kind,
                current_ref: &current_ref,
                incoming_ref: &incoming_ref,
                response_language: if english {
                    "English"
                } else {
                    "Português (Brasil)"
                },
                base_text: entry.base.text.as_deref(),
                current_text: entry.current.text.as_deref(),
                incoming_text: entry.incoming.text.as_deref(),
                result_text: &source,
            },
            &token,
        );
        let title = cx
            .t_with(
                "mergeConflict.agent.sessionTitle",
                &Args::default().with("path", entry.path.clone()),
            )
            .to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.busy = Some(Busy::Agent);
        self.suggestion = None;
        self.agent = Some(AgentRun {
            session_id: None,
            turn_id: None,
            events: Vec::new(),
            status: AgentStatus::Analyzing,
            expanded: true,
            error: None,
            token: token.clone(),
            cancel: cancel.clone(),
        });
        self.set_editor_read_only(true, cx);
        cx.notify();
        let api = self.api();
        let read_events = self.read_events.clone();
        self._agent = Some(cx.spawn(async move |this, cx| {
            // The conversation and its one turn.
            let started = cx
                .background_executor()
                .spawn(async move {
                    let api = api?;
                    let session = api.start_thread(
                        &pick.start,
                        &pick.provider_id,
                        pick.model.as_deref(),
                        Some(&title),
                    )?;
                    let options = TurnOptions {
                        provider_id: None,
                        model: None,
                        effort: Some("medium".into()),
                        approval_policy: Some("ask".into()),
                        plan_mode: false,
                        fast_mode: false,
                        tool_instructions: Some(AGENT_RESOLUTION_TOOL_INSTRUCTIONS.into()),
                        // The conversation just started with its own.
                        provider_runtime: None,
                    };
                    let turn = api.send_turn_for_id(&session, &prompt, &options)?;
                    Ok::<_, ApiError>((api, session, turn))
                })
                .await;
            let (api, session, turn) = match started {
                Ok(started) => started,
                Err(error) => {
                    let _ = this.update(cx, |resolver, cx| {
                        let message = match error {
                            ApiError::DesktopClosed => cx.t("native.desktopClosed").to_string(),
                            error => error.to_string(),
                        };
                        resolver.finish_agent(Outcome::Failed(message), &source, cx)
                    });
                    return;
                }
            };
            let _ = this.update(cx, |resolver, cx| {
                if let Some(run) = resolver.agent.as_mut() {
                    run.session_id = Some(session.clone());
                    run.turn_id = Some(turn.clone());
                }
                cx.notify();
            });
            // `waitForAgentResolutionTurn`
            let deadline = Instant::now() + TIMEOUT;
            let mut abort_sent = false;
            let outcome = loop {
                let (read, id) = (read_events.clone(), session.clone());
                let events = cx
                    .background_executor()
                    .spawn(async move { read(&id) })
                    .await;
                let end = agent_turn_end(&events, &turn);
                let answer = matches!(end, Some(AgentTurnEnd::Completed)).then(|| {
                    project_session(&events, &session)
                        .into_iter()
                        .rev()
                        .find(|message| {
                            message.role == Role::Assistant && !message.content.trim().is_empty()
                        })
                        .map(|message| message.content)
                });
                let alive = this.update(cx, |resolver, cx| {
                    if let Some(run) = resolver.agent.as_mut() {
                        run.events = events;
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
                match end {
                    Some(AgentTurnEnd::Completed) => {
                        break match answer.flatten() {
                            Some(content) => Outcome::Answer(content),
                            None => Outcome::Failed(
                                "O agente concluiu sem retornar uma sugestão textual.".into(),
                            ),
                        };
                    }
                    Some(AgentTurnEnd::Aborted(reason)) => {
                        break Outcome::Failed(agent_aborted_message(reason.as_deref()));
                    }
                    None => {}
                }
                if Instant::now() >= deadline {
                    break Outcome::Failed(
                        "O agente demorou demais para concluir a sugestão de resolução. Tente novamente."
                            .into(),
                    );
                }
                if cancel.load(Ordering::SeqCst) && !abort_sent {
                    abort_sent = true;
                    let (api, id) = (api.clone(), session.clone());
                    if let Err(error) = cx
                        .background_executor()
                        .spawn(async move { api.abort(&id) })
                        .await
                    {
                        break Outcome::Failed(error.to_string());
                    }
                }
                cx.background_executor().timer(POLL).await;
            };
            let _ = this.update(cx, |resolver, cx| {
                resolver.finish_agent(outcome, &source, cx)
            });
        }));
    }

    /// The answer checked and put in the editor, or the run marked failed.
    fn finish_agent(&mut self, outcome: Outcome, source: &str, cx: &mut Context<Self>) {
        self.busy = None;
        self.set_editor_read_only(false, cx);
        let (session, token) = self
            .agent
            .as_ref()
            .map(|run| {
                (
                    run.session_id.clone().unwrap_or_default(),
                    run.token.clone(),
                )
            })
            .unwrap_or_default();
        let result = match outcome {
            Outcome::Failed(message) => Err(message),
            Outcome::Answer(content) => self.apply_answer(&content, source, &token, &session, cx),
        };
        match result {
            Ok(()) => {
                if let Some(run) = self.agent.as_mut() {
                    run.status = AgentStatus::Completed;
                    run.expanded = false;
                    run.error = None;
                }
                Toaster::success(cx.t("mergeConflict.toast.agentApplied"), cx);
            }
            Err(message) => {
                if let Some(run) = self.agent.as_mut() {
                    run.status = AgentStatus::Failed;
                    run.expanded = true;
                    run.error = Some(message.clone());
                }
                Toaster::error(message, cx);
            }
        }
        cx.notify();
    }

    fn apply_answer(
        &mut self,
        content: &str,
        source: &str,
        token: &str,
        session: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(open) = self.open.as_ref() else {
            return Err(String::new());
        };
        let (current, incoming) = (
            open.entry.current.text.clone(),
            open.entry.incoming.text.clone(),
        );
        let (buffer, path) = (open.buffer.clone(), open.entry.path.clone());
        let has_current = current.is_some();
        let suggestion = parse_agent_resolution(content, token)
            .and_then(|suggestion| {
                validate_agent_resolution(suggestion, current.as_deref(), incoming.as_deref())
            })
            .map_err(|error| cx.t(error.message_key()).to_string())?;
        // The result moved while the agent worked: its answer stays in the session.
        if buffer != source {
            return Err(cx.t("mergeConflict.errors.agentResultChanged").to_string());
        }
        self.set_result(suggestion.resolved_content, cx);
        if has_current {
            self.set_view(ResultView::CompareCurrent, cx);
        }
        self.suggestion = Some(AppliedSuggestion {
            path,
            explanation: suggestion.explanation,
            session_id: session.to_string(),
        });
        Ok(())
    }

    /// "Cancelar": the turn aborted on the desktop; the run waits for it.
    pub(crate) fn cancel_agent(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.agent.as_mut() else {
            return;
        };
        if run.status != AgentStatus::Analyzing {
            return;
        }
        run.status = AgentStatus::Cancelling;
        run.cancel.store(true, Ordering::SeqCst);
        cx.notify();
    }

    pub(crate) fn toggle_agent_details(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.agent.as_mut() {
            run.expanded = !run.expanded;
            cx.notify();
        }
    }

    /// "Ver sessão": an unsaved result asks first; the resolver closes.
    pub(crate) fn request_agent_session(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.agent.as_ref().and_then(|run| run.session_id.clone()) else {
            return;
        };
        if self.open.as_ref().is_some_and(super::Open::dirty) {
            self.pending = Some(Pending::OpenAgentSession(session));
            cx.notify();
            return;
        }
        cx.emit(MergeResolverEvent::OpenSession(session));
    }

    fn set_editor_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_ref() {
            let editor = open.editor.clone();
            editor.update(cx, |editor, cx| editor.set_read_only(read_only, cx));
        }
    }
}
