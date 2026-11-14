//! "Resolver com agente": one read-only run in the task's checkout, with
//! no task and no conversation made for it (the run the pull request hub's
//! Reviewer uses). The agent gets the file's three versions and its result,
//! resolves every block together and answers the whole file; that answer is
//! put in the editor and offered back block by block (`proposal`), to
//! accept, refuse or change like any text. Nothing is saved. The editor is
//! read-only while it runs, and the run can be stopped. "Resolver todos"
//! runs it file after file: the open one's answer is offered at once, the
//! others' are kept and offered when each file opens.

use dcc_core::domain::merge_conflict::agent::{
    AgentResolutionInput, AgentResolutionSuggestion, build_agent_resolution_prompt,
    new_resolution_token, parse_agent_resolution, validate_agent_resolution,
};
use dcc_core::domain::merge_conflict::proposal::split_proposal;
use dcc_core::domain::merge_conflict::{
    WorkspaceGitConflictEntry, conflict_initial_result, conflict_text_unavailable,
    parse_conflict_hunks,
};
use gpui::Context;

use super::proposal::{BlockState, Proposal, ProposalBlock};
use super::{Busy, MergeResolver};
use crate::data::desktop_api::ApiError;
use crate::i18n::{Args, Locale, Translate};
use crate::ui::Toaster;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Analyzing,
    Cancelling,
    Failed,
}

/// The run on a file, while it works or after it failed; with files still
/// to go when every conflicted file was asked for.
pub(crate) struct AgentRun {
    pub status: AgentStatus,
    pub error: Option<String>,
    /// Names this run on the desktop (to stop it) and fences its answer.
    pub token: String,
    /// What the agent answered, when the answer could not be used: there
    /// is no conversation to look it up in.
    pub raw: Option<String>,
    pub raw_open: bool,
    /// The file this run is on.
    pub path: String,
    /// The files after it ("Resolver todos"), and how many there were.
    pub queue: Vec<String>,
    pub total: usize,
    /// The ones that came back without a usable answer.
    pub failed: Vec<String>,
}

impl AgentRun {
    /// Which of the files it is on, one-based.
    pub fn position(&self) -> usize {
        self.total.saturating_sub(self.queue.len())
    }
}

/// An answer kept for a file that was not on screen when it came.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeptAnswer {
    /// The file as the agent read it.
    pub source: String,
    pub suggestion: AgentResolutionSuggestion,
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
        let Some(path) = self.open.as_ref().map(|open| open.entry.path.clone()) else {
            return;
        };
        self.start_agent(vec![path], cx);
    }

    /// "Resolver todos os arquivos com agente": every conflicted file that
    /// is text with blocks, the open one first, one run after the other.
    pub(crate) fn resolve_all_with_agent(&mut self, cx: &mut Context<Self>) {
        let open = self.open.as_ref().map(|open| open.entry.path.clone());
        let mut paths: Vec<String> = self
            .conflicts()
            .iter()
            .filter(|entry| !conflict_text_unavailable(entry))
            .filter(|entry| !parse_conflict_hunks(&self.source_of(entry)).is_empty())
            .map(|entry| entry.path.clone())
            .collect();
        paths.sort_by_key(|path| Some(path) != open.as_ref());
        self.start_agent(paths, cx);
    }

    /// The file's text the agent reads: what is in the editor for the
    /// open one, else what git left in it.
    fn source_of(&self, entry: &WorkspaceGitConflictEntry) -> String {
        match &self.open {
            Some(open) if open.entry.path == entry.path => open.buffer.clone(),
            _ => conflict_initial_result(entry),
        }
    }

    fn start_agent(&mut self, mut paths: Vec<String>, cx: &mut Context<Self>) {
        if self.busy.is_some() || paths.is_empty() {
            return;
        }
        if self.request.agent.is_none() {
            Toaster::error(cx.t("mergeConflict.agent.noProvider"), cx);
            return;
        }
        let total = paths.len();
        let path = paths.remove(0);
        self.busy = Some(Busy::Agent);
        self.proposal = None;
        self.agent = Some(AgentRun {
            status: AgentStatus::Analyzing,
            error: None,
            token: String::new(),
            raw: None,
            raw_open: false,
            path,
            queue: paths,
            total,
            failed: Vec::new(),
        });
        self.set_editor_read_only(true, cx);
        self.run_agent(cx);
    }

    /// The run's call for the file it is on.
    fn run_agent(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.agent.as_ref().map(|run| run.path.clone()) else {
            return;
        };
        let (Some(pick), Some(entry)) = (
            self.request.agent.clone(),
            self.conflicts()
                .iter()
                .find(|entry| entry.path == path)
                .cloned(),
        ) else {
            return self.finish_agent(Err(ApiError::Failed(String::new())), "", "", cx);
        };
        let ((_, current_ref), (_, incoming_ref)) = self.labels(cx);
        let english = cx.global::<crate::i18n::I18n>().locale() == Locale::En;
        let token = new_resolution_token();
        let source = self.source_of(&entry);
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
        if let Some(run) = self.agent.as_mut() {
            run.token = token.clone();
        }
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

    /// A run ended: its answer goes to the editor as a proposal (or is
    /// kept for its file), or its failure shows; one stopped by the person
    /// just leaves. With files left, the next one starts.
    pub(crate) fn finish_agent(
        &mut self,
        answered: Result<String, ApiError>,
        source: &str,
        token: &str,
        cx: &mut Context<Self>,
    ) {
        // Another run took its place meanwhile.
        if self.agent.as_ref().map(|run| run.token.as_str()) != Some(token) {
            return;
        }
        let stopped = self
            .agent
            .as_ref()
            .is_some_and(|run| run.status == AgentStatus::Cancelling);
        if stopped {
            self.end_agent(cx);
            self.agent = None;
            Toaster::info(cx.t("mergeConflict.agent.cancelled"), cx);
            cx.notify();
            return;
        }
        let raw = answered.as_ref().ok().cloned();
        let applied = answered
            .map_err(|error| reason(error, cx))
            .and_then(|content| self.take_answer(&content, source, token, cx));
        let Some(run) = self.agent.as_mut() else {
            return;
        };
        let several = run.total > 1;
        if let Err(error) = &applied {
            run.failed.push(run.path.clone());
            // One file: its failure stays on screen, with what was answered.
            if !several {
                run.status = AgentStatus::Failed;
                run.error = Some(error.clone());
                run.raw = raw.filter(|raw| !raw.trim().is_empty());
                self.end_agent(cx);
                cx.notify();
                return;
            }
        }
        if run.queue.is_empty() {
            let (failed, total) = (run.failed.len(), run.total);
            self.end_agent(cx);
            self.agent = None;
            if several {
                let args = Args::default()
                    .with("done", (total - failed).to_string())
                    .with("total", total.to_string());
                Toaster::info(cx.t_with("mergeConflict.agent.allDone", &args), cx);
            }
        } else {
            run.path = run.queue.remove(0);
            self.run_agent(cx);
        }
        cx.notify();
    }

    /// Editing again: no run holds the file.
    fn end_agent(&mut self, cx: &mut Context<Self>) {
        self.busy = None;
        self.set_editor_read_only(false, cx);
    }

    /// The answer checked for the run's file: offered at once when that
    /// file is on screen, kept for when it opens otherwise.
    fn take_answer(
        &mut self,
        content: &str,
        source: &str,
        token: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(path) = self.agent.as_ref().map(|run| run.path.clone()) else {
            return Err(String::new());
        };
        let Some(entry) = self
            .conflicts()
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
        else {
            return Err(String::new());
        };
        let suggestion = parse_agent_resolution(content, token, source)
            .and_then(|suggestion| {
                validate_agent_resolution(
                    suggestion,
                    entry.current.text.as_deref(),
                    entry.incoming.text.as_deref(),
                )
            })
            .map_err(|error| cx.t(error.message_key()).to_string())?;
        let on_screen = self
            .open
            .as_ref()
            .is_some_and(|open| open.entry.path == path);
        if !on_screen {
            self.answers.insert(
                path,
                KeptAnswer {
                    source: source.to_string(),
                    suggestion,
                },
            );
            return Ok(());
        }
        self.offer(suggestion, source, cx)
    }

    /// The answer put in the open file and offered block by block.
    fn offer(
        &mut self,
        suggestion: AgentResolutionSuggestion,
        source: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(open) = self.open.as_ref() else {
            return Err(String::new());
        };
        let path = open.entry.path.clone();
        // The file moved while the agent worked: its answer is for another text.
        if open.buffer != source {
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

    /// A file just opened: the answer kept for it is offered, if the file
    /// is still as the agent read it.
    pub(crate) fn offer_kept(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.open.as_ref().map(|open| open.entry.path.clone()) else {
            return;
        };
        if let Some(kept) = self.answers.remove(&path) {
            let _ = self.offer(kept.suggestion, &kept.source, cx);
        }
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
