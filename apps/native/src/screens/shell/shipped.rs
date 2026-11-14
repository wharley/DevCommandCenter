//! Closing the loop of a task an agent opened
//! (`dcc_core::domain::task_shipped`): when its pull request merged, DCC
//! reads from GitHub whether the merge commit reached production and tells
//! the agent the work started with, in its own conversation — which tells
//! the person and, when the request came from a connector, proposes the
//! reply there as a card.
//!
//! No clock of its own: the merged tasks are looked at when the sidebar's
//! rows change, and the ones waiting for a deployment on the pass
//! `PrWatch` already makes every five minutes — one read of GitHub per
//! waiting task, for half an hour at most. A task the person made is
//! remembered as done and never asked about again.

use std::collections::HashSet;

use chrono::Utc;
use dcc_core::domain::task_shipped::{Outcome, shipped_prompt};
use gpui::Context;

use super::Shell;
use crate::data::agent_triggers::AgentTriggers;
use crate::data::desktop_api::DesktopApi;
use crate::data::shipped::{Entry, Next, ShippedStore, next};
use crate::screens::sidebar::merged::MergedTask;

/// What the shell keeps for it.
pub(super) struct ShippedWatch {
    pub(super) store: ShippedStore,
    /// The tasks being read now.
    reading: HashSet<String>,
}

impl ShippedWatch {
    pub(super) fn new() -> Self {
        // A test never writes the person's file.
        let path = (!cfg!(test)).then(|| crate::prefs::native_file("shipped.json"));
        Self {
            store: ShippedStore::load(path),
            reading: HashSet::new(),
        }
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Shell {
    /// The sidebar's rows changed: a task that just merged starts its wait
    /// when an agent opened it, and is set aside when the person did.
    pub(super) fn shipped_seen(&mut self, cx: &mut Context<Self>) {
        let merged: Vec<MergedTask> = self
            .sidebar
            .read(cx)
            .merged_tasks()
            .into_iter()
            .filter(|task| !self.shipped.store.tasks.contains_key(&task.id))
            .collect();
        if merged.is_empty() || cfg!(test) {
            return;
        }
        for task in &merged {
            // Held here so a second change of the rows does not ask twice.
            self.shipped
                .store
                .set(&task.id, Entry::Waiting { since: now() });
        }
        cx.spawn(async move |shell, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    merged
                        .into_iter()
                        .map(|task| {
                            let origin = crate::data::task_origin::read(&task.id).is_some();
                            (task.id, origin)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = shell.update(cx, |shell, cx| {
                for (task, origin) in found {
                    if !origin {
                        shell.shipped.store.set(&task, Entry::Done { at: now() });
                    }
                }
                shell.shipped_due(cx);
            });
        })
        .detach();
    }

    /// Reads where each waiting task's pull request stands and tells the
    /// agent it started with, once there is something to tell.
    pub(super) fn shipped_due(&mut self, cx: &mut Context<Self>) {
        if !cx.has_global::<AgentTriggers>() {
            return;
        }
        let merged = self.sidebar.read(cx).merged_tasks();
        for (task_id, since) in self.shipped.store.waiting() {
            let Some(task) = merged.iter().find(|task| task.id == task_id).cloned() else {
                continue;
            };
            if !self.shipped.reading.insert(task_id.clone()) {
                continue;
            }
            cx.spawn(async move |shell, cx| {
                let (id, checkout, number) = (task.id.clone(), task.checkout.clone(), task.number);
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        let api =
                            DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                        let shipped = api
                            .pull_request_shipped(&checkout, number)
                            .map_err(|error| error.to_string())?;
                        Ok::<_, String>((shipped, crate::data::task_origin::read(&id)))
                    })
                    .await;
                let _ = shell.update(cx, |shell, cx| {
                    shell.shipped.reading.remove(&task.id);
                    match read {
                        // Merged before DCC kept track: old news, not told.
                        Ok((shipped, _))
                            if shell.shipped.store.is_old(shipped.merged_at.as_deref()) =>
                        {
                            shell.shipped.store.set(&task.id, Entry::Done { at: now() })
                        }
                        Ok((shipped, Some(origin))) => {
                            match next(&shipped.shipped, &since, Utc::now()) {
                                Next::Wait => {}
                                Next::Forget => {
                                    shell.shipped.store.tasks.remove(&task.id);
                                }
                                Next::Tell(outcome) => {
                                    shell.tell_shipped(&task, outcome, origin, cx)
                                }
                            }
                        }
                        // The origin is gone (the agent's conversation was
                        // deleted): nobody to tell.
                        Ok((_, None)) => {
                            shell.shipped.store.set(&task.id, Entry::Done { at: now() })
                        }
                        Err(error) => eprintln!("[dcc-native] shipped {}: {error}", task.id),
                    }
                });
            })
            .detach();
        }
    }

    /// Sends the news to the agent the work started with, unless something
    /// holds it (the quiet hours, its brake): then it stays waiting.
    fn tell_shipped(
        &mut self,
        task: &MergedTask,
        outcome: Outcome,
        origin: crate::data::task_origin::OriginRead,
        cx: &mut Context<Self>,
    ) {
        let Some(first) = origin.origin.steps.first() else {
            self.shipped.store.set(&task.id, Entry::Done { at: now() });
            return;
        };
        let Some(agent) = self
            .sidebar
            .read(cx)
            .agents()
            .iter()
            .find(|agent| agent.id == first.agent_id)
            .cloned()
        else {
            self.shipped.store.set(&task.id, Entry::Done { at: now() });
            return;
        };
        let triggers = cx.global::<AgentTriggers>();
        if triggers
            .quiet_check(Utc::now())
            .and_then(|()| triggers.brake_check(&agent.id))
            .is_err()
        {
            return;
        }
        let pull_request = format!("#{}", task.number);
        let prompt = shipped_prompt(&task.name, &pull_request, &outcome, &origin.origin);
        let Some(send) = self.home_send(&agent, first.room.as_deref(), prompt, cx) else {
            return;
        };
        // Marked before it is sent: a send that fails is not repeated
        // every pass.
        self.shipped.store.set(&task.id, Entry::Done { at: now() });
        let id = task.id.clone();
        cx.background_executor()
            .spawn(async move {
                let sent = DesktopApi::load()
                    .ok_or_else(|| "DCC is not running".to_string())
                    .and_then(|api| super::agent_wake::send_home(&api, send));
                if let Err(error) = sent {
                    eprintln!("[dcc-native] shipped {id}: {error}");
                }
            })
            .detach();
    }
}
