//! The routines' clock (`dcc_core::domain::agent_routine`): once a minute
//! it reads the agents' routines from DCC's database (a small table) into
//! the `Routines` global the team room and the agent's page read, and the
//! shell runs the ones due — in the agent's own conversation, unless the
//! room's pause, the quiet hours or the brake hold it (a held routine waits
//! within its catch-up; past the brake it is marked and skipped). The
//! monthly limit is the desktop's: a refused turn is only announced.

use std::time::Duration;

use chrono::{Local, Utc};
use dcc_core::domain::agent_routine::{Routine, routine_prompt};
use gpui::{App, AppContext, BorrowAppContext, Context, EventEmitter, Task};

use super::Shell;
use crate::data::agent_triggers::{AgentTriggers, Held, Trigger};
use crate::data::desktop_api::DesktopApi;
use crate::data::routines::{Routines, read};
use crate::i18n::{Args, Translate};

const EVERY: Duration = Duration::from_secs(60);

/// The routines due on this tick.
pub struct RoutinesDue(pub Vec<Routine>);

pub struct RoutineClock {
    _tick: Option<Task<()>>,
}

impl EventEmitter<RoutinesDue> for RoutineClock {}

impl RoutineClock {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Tests set the routines themselves; a test never reads the real desktop.
        let tick = (!cfg!(test)).then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    let list = cx.background_executor().spawn(async { read() }).await;
                    let alive = this.update(cx, |_, cx| {
                        let now = Local::now();
                        let due: Vec<Routine> = list
                            .iter()
                            .filter(|routine| routine.due(now).is_some())
                            .cloned()
                            .collect();
                        cx.set_global(Routines { list });
                        if !due.is_empty() {
                            cx.emit(RoutinesDue(due));
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

fn announce(agent: &str, done: &Result<(), String>, cx: &App) {
    let title = cx.t_with(
        "agents.routines.done",
        &Args::default().with("agent", agent.to_string()),
    );
    let what = match done {
        Ok(()) => cx.t("agents.routines.ready").to_string(),
        Err(_) => cx.t("agents.routines.failed").to_string(),
    };
    crate::ui::attention::notify_away(&title, &what);
}

impl Shell {
    /// Runs the routines due that nothing holds.
    pub(super) fn routines_due(&mut self, due: &[Routine], cx: &mut Context<Self>) {
        if !cx.has_global::<AgentTriggers>() {
            return;
        }
        for routine in due {
            let Some(agent) = self
                .sidebar
                .read(cx)
                .agents()
                .iter()
                .find(|agent| agent.id == routine.agent_id)
                .cloned()
            else {
                continue;
            };
            let now = Utc::now();
            let triggers = cx.global::<AgentTriggers>();
            match triggers
                .quiet_check(now)
                .and_then(|()| triggers.brake_check(&agent.id))
            {
                // Paused or quiet: it waits, within its catch-up.
                Err(Held::Paused | Held::Quiet) => continue,
                // Braked: skipped until the person talks to the agent.
                Err(_) => {
                    self.mark_routine(routine, cx);
                    continue;
                }
                Ok(()) => {}
            }
            let Some(send) = self.home_send(&agent, None, routine_prompt(routine), cx) else {
                continue;
            };
            cx.update_global::<AgentTriggers, _>(|triggers, _| {
                triggers.record(&agent.id, Trigger::Routine, &routine.id, now)
            });
            let routine = routine.clone();
            let at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            cx.spawn(async move |_, cx| {
                let name = agent.name.clone();
                let done = cx
                    .background_executor()
                    .spawn(async move {
                        let api =
                            DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                        // Marked first: a turn that fails is not retried every minute.
                        api.routine_ran(&routine.agent_id, &routine.id, &at)
                            .map_err(|error| error.to_string())?;
                        super::agent_wake::send_home(&api, send)
                    })
                    .await;
                let _ = cx.update(|cx| announce(&name, &done, cx));
            })
            .detach();
        }
    }

    /// A routine the brake held: marked, so it waits for its next time.
    fn mark_routine(&self, routine: &Routine, cx: &mut Context<Self>) {
        let (agent, id) = (routine.agent_id.clone(), routine.id.clone());
        let at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        cx.background_executor()
            .spawn(async move {
                if let Some(api) = DesktopApi::load() {
                    let _ = api.routine_ran(&agent, &id, &at);
                }
            })
            .detach();
    }
}

/// The clock and the shell's subscription to it.
pub(super) fn start(cx: &mut Context<Shell>) -> (gpui::Entity<RoutineClock>, gpui::Subscription) {
    let clock = cx.new(RoutineClock::new);
    let due = cx.subscribe(&clock, |shell, _, event: &RoutinesDue, cx| {
        shell.routines_due(&event.0, cx)
    });
    (clock, due)
}
