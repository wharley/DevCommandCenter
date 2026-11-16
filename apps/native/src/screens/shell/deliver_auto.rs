//! Sending a task an agent opened without the person pressing its delivery
//! button, where they granted that for the agent and the project
//! (`data::agent_auto_send`): when the task ends a turn with changes, DCC
//! commits them, pushes its branch and opens the pull request — through
//! the routes the button and the "Criar PR" dialog call, with the same
//! text DCC writes with no model (the commit's subject from the files, the
//! request's title from the task). With its pull request open already the
//! changes are committed and pushed to it.
//!
//! No clock of its own: the tasks are looked at when the sidebar's rows
//! change. Each state of a task's changes is tried once: a refusal (the
//! checks before a push, a branch behind its remote, no forge login) is
//! told to the person and the task waits for them, as before. Only a task
//! in its own worktree, never one with a merge under way, never one that
//! ended before delivery was turned on, and the merge itself is never
//! done here.

use std::collections::HashSet;

use gpui::Context;

use super::Shell;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::desktop_api::{DesktopApi, RequestInput};
use crate::i18n::{Args, Translate};
use crate::screens::sidebar::deliverable::DeliverableTask;
use crate::ui::Toaster;

/// What the shell keeps for it.
#[derive(Default)]
pub(super) struct DeliverWatch {
    /// The states of a task's changes already tried in this run of the app.
    tried: HashSet<String>,
    /// The tasks being sent now.
    sending: HashSet<String>,
}

/// Commits and pushes the task's changes and, without a pull request yet,
/// opens it. Blocking; off the UI thread.
fn send(api: &DesktopApi, task: &DeliverableTask) -> Result<(), String> {
    let text = |error: crate::data::desktop_api::ApiError| error.to_string();
    if task.has_request {
        let commit = api.prepare_commit(&task.id).map_err(text)?;
        return api
            .commit_reviewed(
                &task.id,
                &commit.subject,
                commit.body.as_deref(),
                &commit.staged_fingerprint,
                true,
            )
            .map_err(text);
    }
    let token = api.changes_token(&task.id).map_err(text)?;
    api.create_request_with(
        &task.id,
        &RequestInput {
            title: None,
            body: None,
            draft: false,
            changes_token: Some(token),
        },
    )
    .map_err(text)
}

impl Shell {
    /// The sidebar's rows changed: each task of an agent's that ended a
    /// turn with changes is sent, where its agent may.
    pub(super) fn deliver_due(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) || !cx.has_global::<AgentTriggers>() {
            return;
        }
        let due: Vec<DeliverableTask> = {
            let triggers = cx.global::<AgentTriggers>();
            self.sidebar
                .read(cx)
                .deliverable_tasks()
                .into_iter()
                .filter(|task| {
                    triggers.may_deliver(&task.agent_id, &task.project_id, &task.ended_at)
                })
                .filter(|task| {
                    !self.deliver.tried.contains(&task.key)
                        && !self.deliver.sending.contains(&task.id)
                })
                .collect()
        };
        for task in due {
            self.deliver.tried.insert(task.key.clone());
            self.deliver.sending.insert(task.id.clone());
            let agent = self
                .sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == task.agent_id)
                .map(|agent| agent.name.clone())
                .unwrap_or_default();
            cx.spawn(async move |shell, cx| {
                let sent = {
                    let task = task.clone();
                    cx.background_executor()
                        .spawn(async move {
                            DesktopApi::load()
                                .ok_or_else(|| "DCC is not running".to_string())
                                .and_then(|api| send(&api, &task))
                        })
                        .await
                };
                let _ = shell.update(cx, |shell, cx| {
                    shell.deliver.sending.remove(&task.id);
                    let args = Args::default()
                        .with("agent", agent)
                        .with("task", task.name.clone());
                    match sent {
                        Ok(()) => {
                            let key = if task.has_request {
                                "agents.autoSend.deliver.pushed"
                            } else {
                                "agents.autoSend.deliver.opened"
                            };
                            let text = cx.t_with(key, &args);
                            crate::ui::attention::notify_away(&text, "");
                            Toaster::success(text, cx);
                        }
                        Err(error) => {
                            let text = cx.t_with(
                                "agents.autoSend.deliver.failed",
                                &args.with("error", error),
                            );
                            Toaster::error(text, cx);
                        }
                    }
                });
            })
            .detach();
        }
    }
}
