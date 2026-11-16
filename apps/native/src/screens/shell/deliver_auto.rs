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
//!
//! When the pull request opens and the person wrote what the agent is to
//! do then ("avise no #tech marcando o Giovanni"), DCC tells the agent in
//! the conversation the task was opened from, once the rail has read the
//! pull request: the agent announces it through its connectors, which
//! goes as its replies are granted. The commit and the pull request's own
//! text stay DCC's, with no model.

use std::collections::{HashMap, HashSet};

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
    /// The tasks whose pull request just opened, until the rail reads it
    /// and the agent is told.
    opened: HashMap<String, DeliverableTask>,
}

/// What the agent is told when the pull request of a task it opened is
/// there, with what the person asked it to do then.
pub(super) fn opened_prompt(task: &str, request: &str, note: &str) -> String {
    format!(
        "[DCC] Pull request opened: \"{task}\" — {request}.\n\nThis is DCC, not a message from the person. The task \"{task}\" you opened ended its turn, and DCC committed its changes, pushed them and opened its pull request ({request}) in the person's name. For when a pull request opens, the person left you this: \"{note}\".\n\nDo that now with dcc_propose_reply (it goes as the person set for you), one reply per place: say the pull request is open, with its title and its link, and name who should review as the person said, the way that place marks people (look them up through the connector if you need their ids). If the work came from a request made through a connector, you may also say there, in the same thread, that the pull request is open. Do not open a task, do not pass this on and do not review the pull request yourself. If what the person left cannot be done from here, tell them why in one line."
    )
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
        self.announce_opened(cx);
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
                            if !task.has_request {
                                shell.deliver.opened.insert(task.id.clone(), task.clone());
                            }
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

    /// Tells the agent of each pull request that just opened, once the
    /// rail shows it, when the person wrote what to do then.
    fn announce_opened(&mut self, cx: &mut Context<Self>) {
        let read: Vec<(DeliverableTask, String)> = {
            let sidebar = self.sidebar.read(cx);
            self.deliver
                .opened
                .values()
                .filter_map(|task| {
                    let (number, url) = sidebar.request_of(&task.id)?;
                    Some((task.clone(), url.unwrap_or_else(|| format!("#{number}"))))
                })
                .collect()
        };
        for (task, request) in read {
            self.deliver.opened.remove(&task.id);
            let note = cx.global::<AgentTriggers>().deliver_note(&task.agent_id);
            if note.is_empty() {
                continue;
            }
            let Some(agent) = self
                .sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == task.agent_id)
                .cloned()
            else {
                continue;
            };
            // The room of the conversation the task was opened from.
            let room = agent
                .sessions
                .iter()
                .find(|session| session.session_id == task.session_id)
                .and_then(|session| session.room.clone());
            let prompt = opened_prompt(&task.name, &request, &note);
            self.tell_agent(&agent, room.as_deref(), prompt, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_the_pull_request_and_what_the_person_asked() {
        let prompt = opened_prompt(
            "Corrigir CEP",
            "https://github.com/acme/app/pull/12",
            "avise no #tech marcando o Giovanni",
        );
        assert!(prompt.starts_with(
            "[DCC] Pull request opened: \"Corrigir CEP\" — https://github.com/acme/app/pull/12."
        ));
        assert!(prompt.contains("avise no #tech marcando o Giovanni"));
        assert!(prompt.contains("dcc_propose_reply"));
    }
}
