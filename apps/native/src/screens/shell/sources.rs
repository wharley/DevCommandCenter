//! The sources' clock (`dcc_core::domain::agent_source`): once a minute it
//! reads the agents' sources from DCC's database (a small table) into the
//! `Sources` global the room and the agent's page read, and reads each one
//! that is due — through the desktop, with no model. Only news wakes the
//! agent, in its own conversation; the room's pause, the quiet hours and
//! the brake hold a source unread until they let go, so nothing is lost.

use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use dcc_core::domain::agent_source::{AgentSource, source_prompt};
use gpui::{AppContext, BorrowAppContext, Context, EventEmitter, Task};

use super::Shell;
use crate::data::agent_sources::{Sources, read};
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::desktop_api::DesktopApi;

const EVERY: Duration = Duration::from_secs(60);

/// The sources due on this tick.
pub struct SourcesDue(pub Vec<AgentSource>);

pub struct SourceClock {
    _tick: Option<Task<()>>,
}

impl EventEmitter<SourcesDue> for SourceClock {}

impl SourceClock {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Tests set the sources themselves; a test never reads the real desktop.
        let tick = (!cfg!(test)).then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    let (list, replies) = cx
                        .background_executor()
                        .spawn(async { (read(), crate::data::agent_replies::read()) })
                        .await;
                    let alive = this.update(cx, |_, cx| {
                        let now = Utc::now();
                        let due: Vec<AgentSource> = list
                            .iter()
                            .filter(|source| source.due(now))
                            .cloned()
                            .collect();
                        cx.set_global(Sources { list });
                        // A reply sent from the phone leaves the desktop's room.
                        cx.set_global(crate::data::agent_replies::Replies { list: replies });
                        if !due.is_empty() {
                            cx.emit(SourcesDue(due));
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                    cx.background_executor().timer(EVERY).await;
                }
            })
        });
        Self { _tick: tick }
    }
}

/// The sources being read now, so a slow connector is not read twice.
#[derive(Default)]
struct Reading(HashSet<String>);

impl gpui::Global for Reading {}

impl Shell {
    /// Reads the due sources that nothing holds and wakes their agents with news.
    pub(super) fn sources_due(&mut self, due: &[AgentSource], cx: &mut Context<Self>) {
        if !cx.has_global::<AgentTriggers>() {
            return;
        }
        for source in due {
            let Some(agent) = self
                .sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == source.agent_id)
                .cloned()
            else {
                continue;
            };
            let now = Utc::now();
            let triggers = cx.global::<AgentTriggers>();
            // Held: left unread until the hold lets go, so no news is lost.
            if triggers
                .quiet_check(now)
                .and_then(|()| triggers.brake_check(&agent.id))
                .is_err()
            {
                continue;
            }
            if !cx.default_global::<Reading>().0.insert(source.id.clone()) {
                continue;
            }
            let Some(send) = self.home_send(&agent, source.room.as_deref(), String::new(), cx)
            else {
                cx.default_global::<Reading>().0.remove(&source.id);
                continue;
            };
            let source = source.clone();
            cx.spawn(async move |_, cx| {
                let id = source.id.clone();
                let woke = cx
                    .background_executor()
                    .spawn(async move {
                        let api =
                            DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                        let read = api
                            .read_source(&source)
                            .map_err(|error| error.to_string())?;
                        if read.news.is_empty() {
                            return Ok(None);
                        }
                        let mut send = send;
                        send.prompt = source_prompt(&source, &read.news);
                        super::agent_wake::send_home(&api, send)?;
                        Ok::<_, String>(Some(source))
                    })
                    .await;
                let _ = cx.update(|cx| {
                    cx.default_global::<Reading>().0.remove(&id);
                    match woke {
                        Ok(Some(source)) => cx.update_global::<AgentTriggers, _>(|triggers, _| {
                            triggers.record(
                                &source.agent_id,
                                Trigger::Source,
                                &source.id,
                                Utc::now(),
                            )
                        }),
                        Ok(None) => {}
                        Err(error) => eprintln!("[dcc-native] source {id}: {error}"),
                    }
                });
            })
            .detach();
        }
    }
}

/// The clock and the shell's subscription to it.
pub(super) fn start(cx: &mut Context<Shell>) -> (gpui::Entity<SourceClock>, gpui::Subscription) {
    let clock = cx.new(SourceClock::new);
    let due = cx.subscribe(&clock, |shell, _, event: &SourcesDue, cx| {
        shell.sources_due(&event.0, cx)
    });
    (clock, due)
}
