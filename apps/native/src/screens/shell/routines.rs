//! The routines on screen (`dcc_core::domain::agent_routine`): once a minute
//! it reads the agents' routines from DCC's database (a small table) into
//! the `Routines` global the team room and the agent's page read. Running
//! them at their time is the server's (`dcc_server::http_api::agent_clock`),
//! with or without this window; here "Rodar agora" asks it, and a routine it
//! ran is announced when the window is away.

use std::time::Duration;

use chrono::DateTime;
use dcc_core::domain::agent_routine::Routine;
use gpui::{App, AppContext, Context, Global, Task};

use super::Shell;
use crate::data::agent_triggers::{AgentTriggers, Trigger};
use crate::data::desktop_api::DesktopApi;
use crate::data::routines::{Routines, read};
use crate::i18n::{Args, Translate};
use crate::ui::Toaster;

const EVERY: Duration = Duration::from_secs(60);

pub struct RoutineClock {
    _tick: Option<Task<()>>,
}

impl RoutineClock {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Tests set the routines themselves; a test never reads the real desktop.
        let tick = (!cfg!(test)).then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    let list = cx.background_executor().spawn(async { read() }).await;
                    let alive = this.update(cx, |_, cx| cx.set_global(Routines { list }));
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

/// The newest routine run already announced; none until the first look,
/// which only takes note.
#[derive(Default)]
struct Announced(Option<String>);

impl Global for Announced {}

impl Shell {
    /// The server ran a routine, or woke an agent for a failing check: say
    /// so when the window is away. Called once the triggers' file is read
    /// again.
    pub(super) fn announce_routine_runs(&self, cx: &mut App) {
        let runs: Vec<(String, String, Trigger)> = cx
            .global::<AgentTriggers>()
            .runs()
            .iter()
            .filter(|run| matches!(run.trigger, Trigger::Routine | Trigger::CiFailed))
            .map(|run| (run.agent_id.clone(), run.at.clone(), run.trigger))
            .collect();
        let newest = runs
            .iter()
            .map(|(_, at, _)| at.clone())
            .max_by(|a, b| later(a, b));
        let seen = cx.default_global::<Announced>().0.clone();
        if let Some(seen) = &seen {
            for (agent_id, at, trigger) in &runs {
                if later(at, seen) == std::cmp::Ordering::Greater {
                    let name = self
                        .sidebar
                        .read(cx)
                        .agents()
                        .iter()
                        .find(|agent| &agent.id == agent_id)
                        .map(|agent| agent.name.clone());
                    if let Some(name) = name {
                        announce(&name, *trigger, cx);
                    }
                }
            }
        }
        if newest.is_some() {
            cx.default_global::<Announced>().0 = newest;
        }
    }

    /// "Rodar agora": the server runs it unless a hold keeps it.
    pub(super) fn run_routine_now(&mut self, routine: Routine, cx: &mut Context<Self>) {
        cx.spawn(async move |_, cx| {
            let started = cx
                .background_executor()
                .spawn(async move {
                    let api = DesktopApi::load().ok_or_else(|| "DCC is not running".to_string())?;
                    api.run_routine_now(&routine.agent_id, &routine.id)
                        .map_err(|error| error.to_string())
                })
                .await;
            cx.update(|cx| match started {
                Ok(true) => {}
                Ok(false) => Toaster::warning(cx.t("agents.routines.held"), cx),
                Err(error) => Toaster::error(error, cx),
            });
        })
        .detach();
    }
}

/// `a` after `b`, by time (both RFC 3339).
fn later(a: &str, b: &str) -> std::cmp::Ordering {
    match (
        DateTime::parse_from_rfc3339(a),
        DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

fn announce(agent: &str, trigger: Trigger, cx: &App) {
    let (done, ready) = match trigger {
        Trigger::CiFailed => (
            "agents.triggers.ciFailed.done",
            "agents.triggers.ciFailed.ready",
        ),
        _ => ("agents.routines.done", "agents.routines.ready"),
    };
    let title = cx.t_with(done, &Args::default().with("agent", agent.to_string()));
    crate::ui::attention::notify_away(&title, &cx.t(ready));
}

/// The clock that keeps the list on screen.
pub(super) fn start(cx: &mut Context<Shell>) -> gpui::Entity<RoutineClock> {
    cx.new(RoutineClock::new)
}
