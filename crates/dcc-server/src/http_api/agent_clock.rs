//! The agents' clock, in the server: routines at their time and sources
//! read when due, with or without a screen (`docs/PLANO_MAQUINA_SEM_TELA`,
//! slice 4). It was two clocks in the native app, so nothing woke an agent
//! on a machine with no app, and on a Mac it now runs once, here, in the
//! process that hosts the server.
//!
//! What it learned from Kuvo's `kuvo-wake` and agentes.chat's routines
//! (ADR 0053):
//! - it sleeps until the next routine or source is due, at most a minute,
//!   instead of asking every minute;
//! - a source whose read fails waits its own interval before trying again,
//!   then twice as long each time it fails in a row, up to six hours, so a
//!   connector that broke (an expired token) is not called all day; a
//!   failure is never news;
//! - a routine is marked as run before it is sent, so one whose turn fails
//!   is not sent again every minute (one run per time, as both do);
//! - it reads a source with no model and wakes the agent only with news.
//!
//! "Pausar todos" and the quiet hours hold both (a routine waits within its
//! catch-up, a source stays unread); the brake skips a routine to its next
//! time and keeps a source unread. The monthly limit is the turn's own.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use dcc_core::domain::agent_routine::{routine_prompt, Routine};
use dcc_core::domain::agent_source::{source_prompt, AgentSource};
use dcc_core::domain::team_room::RouteReason;
use dcc_infra::db::SqliteSessionRepo;
use dcc_tauri::agent_triggers::{AgentTriggers, Held, Trigger};
use tokio::sync::RwLock;

use super::mobile_room_send::{deliver, destination_of, room_name};
use super::room_feed::read_team;
use super::HttpBackend;
use crate::http_config::HttpConfig;

/// The longest the clock sleeps: a routine or source added meanwhile is
/// seen within it.
const LONGEST_SLEEP: Duration = Duration::from_secs(60);
const SHORTEST_SLEEP: Duration = Duration::from_secs(1);
/// The longest a failing source waits between tries.
const LONGEST_BACKOFF_MINUTES: i64 = 6 * 60;

/// A source whose reads fail: how many in a row, and when it is tried again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Failing {
    in_row: u32,
    next_try: DateTime<Utc>,
}

/// How long a source that failed `in_row` times in a row waits: its own
/// interval, then twice as long each time, up to six hours.
fn backoff(every_minutes: u32, in_row: u32) -> chrono::Duration {
    let every = i64::from(every_minutes.max(1));
    let doubled = every.saturating_mul(1_i64 << in_row.saturating_sub(1).min(16));
    chrono::Duration::minutes(doubled.min(LONGEST_BACKOFF_MINUTES))
}

/// When a source is next due: its interval after its last read, or its next
/// try when it is failing.
fn source_next(source: &AgentSource, failing: Option<&Failing>) -> Option<DateTime<Utc>> {
    if !source.enabled || source.proposed {
        return None;
    }
    let after_read = source
        .last_checked_at
        .as_deref()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| {
            at.with_timezone(&Utc) + chrono::Duration::minutes(i64::from(source.every_minutes))
        });
    match (after_read, failing) {
        (Some(read), Some(failing)) => Some(read.max(failing.next_try)),
        (None, Some(failing)) => Some(failing.next_try),
        // Never read: now.
        (read, None) => Some(read.unwrap_or(DateTime::<Utc>::MIN_UTC)),
    }
}

/// How long to sleep until the soonest of `dues`, within the bounds.
fn sleep_until(now: DateTime<Utc>, dues: impl IntoIterator<Item = DateTime<Utc>>) -> Duration {
    dues.into_iter()
        .min()
        .map(|due| (due - now).to_std().unwrap_or(SHORTEST_SLEEP))
        .unwrap_or(LONGEST_SLEEP)
        .clamp(SHORTEST_SLEEP, LONGEST_SLEEP)
}

pub(crate) struct Clock {
    config: Arc<RwLock<HttpConfig>>,
    backend: Arc<HttpBackend>,
    triggers: Mutex<AgentTriggers>,
    /// Sources being read now, so a slow connector is not read twice.
    reading: Mutex<HashSet<String>>,
    failing: Mutex<HashMap<String, Failing>>,
    /// Said once: there are sources but no connectors to read them.
    said_no_connectors: Mutex<bool>,
}

/// Starts the clock for the life of the process.
pub(crate) fn start(config: Arc<RwLock<HttpConfig>>, backend: Arc<HttpBackend>) {
    let triggers_file = backend
        .workspace
        .db_path
        .parent()
        .map(|dir| dir.join("agent-triggers.json"))
        .unwrap_or_else(|| PathBuf::from("agent-triggers.json"));
    let clock = Arc::new(Clock {
        config,
        backend,
        triggers: Mutex::new(AgentTriggers::load(triggers_file)),
        reading: Mutex::default(),
        failing: Mutex::default(),
        said_no_connectors: Mutex::new(false),
    });
    let _ = clock.backend.agent_clock.set(clock.clone());
    crate::runtime::spawn(async move {
        loop {
            let wait = clock.clone().tick().await;
            tokio::time::sleep(wait).await;
        }
    });
}

impl Clock {
    /// Runs what is due and says how long to sleep.
    async fn tick(self: Arc<Self>) -> Duration {
        let now = Utc::now();
        if let Ok(mut triggers) = self.triggers.lock() {
            triggers.refresh();
        }
        let routines = self
            .backend
            .sessions
            .list_agent_routines()
            .unwrap_or_default();
        let sources = self
            .backend
            .sessions
            .list_agent_sources()
            .unwrap_or_default();
        for routine in &routines {
            if routine.due(now.with_timezone(&Local)).is_some() {
                let _ = self.run_routine(routine, now);
            }
        }
        for source in &sources {
            if self.source_due(source, now) {
                let _ = self.read_source(source, now);
            }
        }
        let local = now.with_timezone(&Local);
        let failing = self
            .failing
            .lock()
            .map(|map| map.clone())
            .unwrap_or_default();
        let dues = routines
            .iter()
            .filter_map(|routine| routine.next_run(local))
            .map(|at| at.with_timezone(&Utc))
            .chain(
                sources
                    .iter()
                    .filter_map(|source| source_next(source, failing.get(&source.id))),
            );
        sleep_until(now, dues)
    }

    /// Whether the pause, the quiet hours or the brake hold `agent_id` now.
    fn held(&self, agent_id: &str, now: DateTime<Utc>) -> Result<(), Held> {
        let triggers = self.triggers.lock().map_err(|_| Held::Paused)?;
        triggers
            .quiet_check(now)
            .and_then(|()| triggers.brake_check(agent_id))
    }

    /// Sends the routine now unless a hold keeps it, which it returns.
    fn run_routine(self: &Arc<Self>, routine: &Routine, now: DateTime<Utc>) -> Result<(), Held> {
        match self.held(&routine.agent_id, now) {
            // It waits, within its catch-up.
            Err(held @ (Held::Paused | Held::Quiet)) => return Err(held),
            // Braked: to its next time.
            Err(held) => {
                self.mark_ran(routine, now);
                return Err(held);
            }
            Ok(()) => {}
        }
        // Marked first: a turn that fails is not sent again every minute.
        self.mark_ran(routine, now);
        if let Ok(mut triggers) = self.triggers.lock() {
            triggers.record(&routine.agent_id, Trigger::Routine, &routine.id, now);
        }
        let clock = self.clone();
        let routine = routine.clone();
        crate::runtime::spawn(async move {
            if let Err(error) = clock
                .send_home(&routine.agent_id, None, routine_prompt(&routine))
                .await
            {
                eprintln!("[DCC][agents] routine {}: {error}", routine.id);
            }
        });
        Ok(())
    }

    fn mark_ran(&self, routine: &Routine, now: DateTime<Utc>) {
        let at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Err(error) =
            self.backend
                .sessions
                .mark_agent_routine_ran(&routine.agent_id, &routine.id, &at)
        {
            eprintln!("[DCC][agents] routine {}: {error}", routine.id);
        }
    }

    /// Due by its interval and not waiting after a failure.
    fn source_due(&self, source: &AgentSource, now: DateTime<Utc>) -> bool {
        source.due(now)
            && self
                .failing
                .lock()
                .ok()
                .and_then(|map| map.get(&source.id).copied())
                .is_none_or(|failing| failing.next_try <= now)
    }

    /// Reads the source now, waking its agent with news, unless a hold keeps
    /// it (returned) or it is being read already.
    fn read_source(self: &Arc<Self>, source: &AgentSource, now: DateTime<Utc>) -> Result<(), Held> {
        // Held: left unread until the hold lets go, so no news is lost.
        self.held(&source.agent_id, now)?;
        let Some(gateway) = self
            .backend
            .app
            .get()
            .and_then(|app| app.connectors())
            .map(|(_, gateway)| gateway)
        else {
            if let Ok(mut said) = self.said_no_connectors.lock() {
                if !*said {
                    *said = true;
                    eprintln!("[DCC][agents] sources wait: connectors are not available");
                }
            }
            return Ok(());
        };
        if !self
            .reading
            .lock()
            .is_ok_and(|mut reading| reading.insert(source.id.clone()))
        {
            return Ok(());
        }
        let clock = self.clone();
        let source = source.clone();
        crate::runtime::spawn(async move {
            let read =
                dcc_tauri::source_read::read_source(&clock.backend.sessions, &gateway, &source.id)
                    .await;
            let woke = match read {
                Ok(read) => {
                    if let Ok(mut failing) = clock.failing.lock() {
                        failing.remove(&source.id);
                    }
                    if read.news.is_empty() {
                        Ok(false)
                    } else {
                        clock
                            .send_home(
                                &source.agent_id,
                                source.room.clone(),
                                source_prompt(&source, &read.news),
                            )
                            .await
                            .map(|()| true)
                    }
                }
                Err(error) => {
                    let in_row = clock.fail(&source, Utc::now());
                    Err(format!("{error} (falhou {in_row} vez(es) seguidas)"))
                }
            };
            if let Ok(mut reading) = clock.reading.lock() {
                reading.remove(&source.id);
            }
            match woke {
                Ok(true) => {
                    if let Ok(mut triggers) = clock.triggers.lock() {
                        triggers.record(&source.agent_id, Trigger::Source, &source.id, Utc::now());
                    }
                }
                Ok(false) => {}
                Err(error) => eprintln!("[DCC][agents] source {}: {error}", source.id),
            }
        });
        Ok(())
    }

    /// One more failure in a row; returns how many.
    fn fail(&self, source: &AgentSource, now: DateTime<Utc>) -> u32 {
        let Ok(mut failing) = self.failing.lock() else {
            return 0;
        };
        let in_row = failing.get(&source.id).map_or(0, |f| f.in_row) + 1;
        failing.insert(
            source.id.clone(),
            Failing {
                in_row,
                next_try: now + backoff(source.every_minutes, in_row),
            },
        );
        in_row
    }

    /// Wakes `agent_id` in its own conversation in `room` (none: Geral), or
    /// a new one there; queued when that conversation is busy.
    async fn send_home(
        &self,
        agent_id: &str,
        room: Option<String>,
        prompt: String,
    ) -> Result<(), String> {
        let db_path = self.backend.workspace.db_path.clone();
        let (agent, place) = (agent_id.to_string(), room.clone());
        let to = tokio::task::spawn_blocking(move || {
            let room_name = match room_name(&db_path, place.as_deref())? {
                Ok(name) => name,
                Err(_) => return Ok(None),
            };
            let repo =
                SqliteSessionRepo::open_read_only(&db_path).map_err(|error| error.to_string())?;
            let team = read_team(&repo)?;
            let Some(agent) = team.agents.iter().find(|candidate| candidate.id == agent) else {
                return Ok(None);
            };
            let session = team.conversation(&agent.id, place.as_deref());
            Ok::<_, String>(Some(destination_of(
                &team,
                agent,
                RouteReason::Default,
                session,
                room_name,
            )))
        })
        .await
        .map_err(|error| error.to_string())??;
        let Some(to) = to else {
            return Err("the agent or its room is gone".to_string());
        };
        deliver(
            self.config.clone(),
            &self.backend,
            &to,
            room.as_deref(),
            &prompt,
            None,
        )
        .await
        .map(|_| ())
        .map_err(|error| error.message().to_string())
    }
}

/// Runs `change` on the triggers' state, caught up with the file: the
/// clock's own copy when it runs, else one read now.
pub(super) fn with_triggers<T>(
    backend: &HttpBackend,
    change: impl FnOnce(&mut AgentTriggers) -> T,
) -> Option<T> {
    if let Some(clock) = backend.agent_clock.get() {
        let mut triggers = clock.triggers.lock().ok()?;
        triggers.refresh();
        return Some(change(&mut triggers));
    }
    let dir = backend.workspace.db_path.parent()?;
    Some(change(&mut AgentTriggers::load(
        dir.join("agent-triggers.json"),
    )))
}

/// The person talked to `agent_id` (from the phone's room, or any client
/// through the server): its brake starts over, as the app's own room does.
pub(super) fn attended(backend: &HttpBackend, agent_id: &str) {
    let now = Utc::now();
    if let Some(clock) = backend.agent_clock.get() {
        if let Ok(mut triggers) = clock.triggers.lock() {
            triggers.attend(agent_id, now);
        }
        return;
    }
    if let Some(dir) = backend.workspace.db_path.parent() {
        AgentTriggers::load(dir.join("agent-triggers.json")).attend(agent_id, now);
    }
}

/// What a hold is called in an answer.
fn held_name(held: Held) -> &'static str {
    match held {
        Held::Off => "off",
        Held::Paused => "paused",
        Held::Quiet => "quiet",
        Held::Unattended => "unattended",
        Held::DailyLimit => "dailyLimit",
        Held::TargetLimit => "targetLimit",
    }
}

fn the_clock(backend: &HttpBackend) -> Result<Arc<Clock>, super::HttpApiError> {
    backend.agent_clock.get().cloned().ok_or_else(|| {
        super::HttpApiError::ServiceUnavailable("the agents' clock is not running".into())
    })
}

fn started(result: Result<(), Held>) -> axum::Json<serde_json::Value> {
    axum::Json(match result {
        Ok(()) => serde_json::json!({ "started": true }),
        Err(held) => serde_json::json!({ "started": false, "held": held_name(held) }),
    })
}

/// "Rodar agora": the routine now, whatever its time; the pause, the quiet
/// hours and the brake still hold it.
pub(super) async fn run_routine_now(
    axum::extract::State(backend): axum::extract::State<Arc<HttpBackend>>,
    axum::extract::Path((agent_id, routine_id)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<serde_json::Value>, super::HttpApiError> {
    let clock = the_clock(&backend)?;
    let routine = backend
        .sessions
        .list_agent_routines()
        .map_err(|error| super::HttpApiError::internal(error.to_string()))?
        .into_iter()
        .find(|routine| routine.agent_id == agent_id.trim() && routine.id == routine_id.trim())
        .ok_or_else(|| super::HttpApiError::not_found("routine not found"))?;
    Ok(started(clock.run_routine(&routine, Utc::now())))
}

/// "Ler agora": the source now, whatever its interval or failures; the
/// pause, the quiet hours and the brake still hold it.
pub(super) async fn read_source_now(
    axum::extract::State(backend): axum::extract::State<Arc<HttpBackend>>,
    axum::extract::Path((agent_id, source_id)): axum::extract::Path<(String, String)>,
) -> Result<axum::Json<serde_json::Value>, super::HttpApiError> {
    let clock = the_clock(&backend)?;
    let source = backend
        .sessions
        .list_agent_sources()
        .map_err(|error| super::HttpApiError::internal(error.to_string()))?
        .into_iter()
        .find(|source| source.agent_id == agent_id.trim() && source.id == source_id.trim())
        .ok_or_else(|| super::HttpApiError::not_found("source not found"))?;
    Ok(started(clock.read_source(&source, Utc::now())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn source(every: u32, last: Option<&str>) -> AgentSource {
        AgentSource {
            id: "s".into(),
            agent_id: "sup".into(),
            connector_id: "slack".into(),
            connector: "Slack".into(),
            watch: "menções".into(),
            tool: "search".into(),
            arguments: "{}".into(),
            every_minutes: every,
            enabled: true,
            proposed: false,
            last_checked_at: last.map(str::to_string),
            created_at: "2026-11-18T09:00:00Z".into(),
            room: None,
        }
    }

    #[test]
    fn a_failing_source_waits_longer_each_time_up_to_six_hours() {
        assert_eq!(backoff(15, 1), chrono::Duration::minutes(15));
        assert_eq!(backoff(15, 2), chrono::Duration::minutes(30));
        assert_eq!(backoff(15, 3), chrono::Duration::minutes(60));
        assert_eq!(backoff(1, 1), chrono::Duration::minutes(1));
        assert_eq!(backoff(15, 40), chrono::Duration::hours(6));
        assert_eq!(backoff(0, 1), chrono::Duration::minutes(1));
    }

    #[test]
    fn a_source_is_next_due_after_its_interval_or_its_next_try() {
        let read = source(15, Some("2026-11-18T10:00:00Z"));
        assert_eq!(source_next(&read, None), Some(at("2026-11-18T10:15:00Z")));
        let failing = Failing {
            in_row: 2,
            next_try: at("2026-11-18T11:00:00Z"),
        };
        assert_eq!(
            source_next(&read, Some(&failing)),
            Some(at("2026-11-18T11:00:00Z"))
        );
        // Never read: now; off or proposed: never.
        assert_eq!(
            source_next(&source(15, None), None),
            Some(DateTime::<Utc>::MIN_UTC)
        );
        let mut off = read.clone();
        off.enabled = false;
        assert_eq!(source_next(&off, None), None);
    }

    #[test]
    fn the_clock_sleeps_until_the_soonest_within_a_minute() {
        let now = at("2026-11-18T10:00:00Z");
        assert_eq!(sleep_until(now, []), LONGEST_SLEEP);
        assert_eq!(
            sleep_until(
                now,
                [at("2026-11-18T10:00:20Z"), at("2026-11-18T10:05:00Z")]
            ),
            Duration::from_secs(20)
        );
        assert_eq!(
            sleep_until(now, [at("2026-11-18T12:00:00Z")]),
            LONGEST_SLEEP
        );
        // Overdue: right away, not in a tight loop.
        assert_eq!(
            sleep_until(now, [at("2026-11-18T09:00:00Z")]),
            SHORTEST_SLEEP
        );
    }
}
