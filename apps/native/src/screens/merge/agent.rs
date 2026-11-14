//! "Resolver com agente": one read-only run in the task's checkout, with
//! no task and no conversation made for it (the run the pull request hub's
//! Reviewer uses). The agent gets the file's three versions and its result,
//! resolves every block together and answers the whole file; that answer is
//! put in the editor and offered back block by block (`proposal`), to
//! accept, refuse or change like any text. Nothing is saved. The editor is
//! read-only while it runs, and the run can be stopped.

use dcc_core::domain::merge_conflict::agent::{
    AgentResolutionInput, build_agent_resolution_prompt, new_resolution_token,
    parse_agent_resolution, validate_agent_resolution,
};
use dcc_core::domain::merge_conflict::proposal::split_proposal;
use gpui::Context;

use super::proposal::{BlockState, Proposal, ProposalBlock};
use super::{Busy, MergeResolver};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Locale, Translate};
use crate::ui::Toaster;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Analyzing,
    Cancelling,
    Failed,
}

/// The run on the open file, while it works or after it failed.
pub(crate) struct AgentRun {
    pub status: AgentStatus,
    pub error: Option<String>,
    /// Names this run on the desktop (to stop it) and fences its answer.
    pub token: String,
    /// What the agent answered, when the answer could not be used: there
    /// is no conversation to look it up in.
    pub raw: Option<String>,
    pub raw_open: bool,
}

fn reason(error: ApiError, cx: &gpui::App) -> String {
    match error {
        ApiError::Failed(message) if !message.trim().is_empty() => message,
        other => other.text(cx),
    }
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
            Toaster::error(cx.t("mergeConflict.agent.noProvider"), cx);
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
        self.busy = Some(Busy::Agent);
        self.proposal = None;
        self.agent = Some(AgentRun {
            status: AgentStatus::Analyzing,
            error: None,
            token: token.clone(),
            raw: None,
            raw_open: false,
        });
        self.set_editor_read_only(true, cx);
        cx.notify();
        let api = self.api();
        let root = self.request.root.clone();
        self._agent = Some(cx.spawn(async move |this, cx| {
            let run = token.clone();
            let answered = cx
                .background_executor()
                .spawn(async move {
                    api?.conflict_agent(
                        &root,
                        &pick.provider_id,
                        pick.model.as_deref(),
                        &prompt,
                        &run,
                    )
                })
                .await;
            let _ = this.update(cx, |resolver, cx| {
                resolver.finish_agent(answered, &source, &token, cx)
            });
        }));
    }

    /// The run ended: its answer goes to the editor as a proposal, or its
    /// failure shows; one stopped by the person just leaves.
    pub(crate) fn finish_agent(
        &mut self,
        answered: Result<String, ApiError>,
        source: &str,
        token: &str,
        cx: &mut Context<Self>,
    ) {
        // Another run, or another file, took its place meanwhile.
        if self.agent.as_ref().map(|run| run.token.as_str()) != Some(token) {
            return;
        }
        self.busy = None;
        self.set_editor_read_only(false, cx);
        let stopped = self
            .agent
            .as_ref()
            .is_some_and(|run| run.status == AgentStatus::Cancelling);
        if stopped {
            self.agent = None;
            Toaster::info(cx.t("mergeConflict.agent.cancelled"), cx);
            cx.notify();
            return;
        }
        let raw = answered.as_ref().ok().cloned();
        let applied = answered
            .map_err(|error| reason(error, cx))
            .and_then(|content| self.apply_answer(&content, source, token, cx));
        match applied {
            Ok(()) => self.agent = None,
            Err(error) => {
                if let Some(run) = self.agent.as_mut() {
                    run.status = AgentStatus::Failed;
                    run.error = Some(error);
                    run.raw = raw.filter(|raw| !raw.trim().is_empty());
                }
            }
        }
        cx.notify();
    }

    fn apply_answer(
        &mut self,
        content: &str,
        source: &str,
        token: &str,
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
        let suggestion = parse_agent_resolution(content, token, source)
            .and_then(|suggestion| {
                validate_agent_resolution(suggestion, current.as_deref(), incoming.as_deref())
            })
            .map_err(|error| cx.t(error.message_key()).to_string())?;
        // The file moved while the agent worked: its answer is for another text.
        if buffer != source {
            return Err(cx.t("mergeConflict.errors.agentResultChanged").to_string());
        }
        // By block where the text between the blocks came back as it was;
        // else one proposal for the file.
        let mut notes = suggestion.block_notes.into_iter();
        let blocks = match split_proposal(source, &suggestion.resolved_content) {
            Some(blocks) => {
                let single = blocks.len() == 1;
                blocks
                    .into_iter()
                    .map(|block| ProposalBlock {
                        original: block.original,
                        proposed: block.proposed,
                        // Notes follow the file's blocks; a run of blocks
                        // proposed as one takes the first of them.
                        note: notes.next().filter(|note| !note.is_empty() && !single),
                        state: BlockState::Pending,
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        self.set_result(suggestion.resolved_content, cx);
        self.proposal = Some(Proposal {
            path,
            explanation: suggestion.explanation,
            before: source.to_string(),
            blocks,
            at: 0,
        });
        self.decorate(cx);
        // What it wrote comes into view: the first block it proposes.
        let first = match (&self.proposal, &self.open) {
            (Some(proposal), Some(open)) => proposal
                .lines(&open.buffer)
                .first()
                .map(|(index, _)| *index),
            _ => None,
        };
        if let Some(first) = first {
            self.reveal_proposed(first, cx);
        }
        Ok(())
    }

    /// "Cancelar": the run is stopped on the desktop, and ends here when
    /// that comes back.
    pub(crate) fn cancel_agent(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.agent.as_mut() else {
            return;
        };
        if run.status != AgentStatus::Analyzing {
            return;
        }
        run.status = AgentStatus::Cancelling;
        let token = run.token.clone();
        let api = self.api();
        cx.background_executor()
            .spawn(async move {
                if let Ok(api) = api {
                    let _ = api.cancel_conflict_agent(&token);
                }
            })
            .detach();
        cx.notify();
    }

    /// "Ver resposta": what the agent wrote, under the failure.
    pub(crate) fn toggle_agent_answer(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.agent.as_mut() {
            run.raw_open = !run.raw_open;
            cx.notify();
        }
    }

    /// The failure read: its strip leaves.
    pub(crate) fn dismiss_agent_error(&mut self, cx: &mut Context<Self>) {
        if self
            .agent
            .as_ref()
            .is_some_and(|run| run.status == AgentStatus::Failed)
        {
            self.agent = None;
            cx.notify();
        }
    }

    fn set_editor_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_ref() {
            let editor = open.editor.clone();
            editor.update(cx, |editor, cx| editor.set_read_only(read_only, cx));
        }
    }
}
