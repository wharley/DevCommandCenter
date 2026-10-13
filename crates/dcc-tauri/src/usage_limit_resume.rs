//! Continuing a task by itself once its usage limit resets ("Continuar
//! sozinho se o limite acabar").
//!
//! Off by default: a global preference (`usage-limit-settings.json` in the
//! app data dir) sets the choice new limits start with, and each task can
//! change its own (`SessionEventKind::UsageLimitAutoResumeChanged`). The
//! limit itself, its reset time and the automatic resumes already spent are
//! session events (`dcc_core::domain::usage_limit`), so after a restart the
//! scheduler only has to look at the sessions that ever stopped on a limit;
//! a reset that passed while DCC was closed resumes at the next tick, with
//! the slack.
//!
//! On the reset plus a random 1–3 minutes, the scheduler sends the fixed
//! continuation turn with the access level of the turn that stopped; when it
//! completes, the session's queue dispatches as after any turn. At most
//! `MAX_AUTO_RESUMES` automatic resumes per session in 12 hours; past that a
//! new stop waits for the person. DCC never spends a paid reset credit
//! (Codex `account/rateLimitResetCredit/consume`) on its own: only the
//! person does, from the usage dialog.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dcc_core::domain::session::{SessionId, TurnId};
use dcc_core::domain::usage_limit::{
    auto_resume_due_at, SessionUsageLimit, RESUME_SLACK_MAX_SECS, RESUME_SLACK_MIN_SECS,
};
use dcc_core::ports::events::CoreEvent;
use dcc_core::ports::EventBus;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::state::SessionCommandState;

pub const SETTINGS_FILE_NAME: &str = "usage-limit-settings.json";
/// How often waiting limits are checked when nothing is due sooner.
pub const CHECK_EVERY: Duration = Duration::from_secs(15);
/// After a resume fails to start, wait this long before trying again.
const RETRY_AFTER_FAILURE_SECS: i64 = 300;
/// Failed starts of the same resume before the scheduler lets it go (the
/// task then shows the limit and the person resumes it).
const MAX_START_FAILURES: u32 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitSettings {
    /// The choice new limits start with when the task made none.
    #[serde(default)]
    pub auto_resume: bool,
}

pub fn load_settings(app_data_dir: &Path) -> UsageLimitSettings {
    std::fs::read(app_data_dir.join(SETTINGS_FILE_NAME))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UsageLimitSettings>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(app_data_dir: &Path, settings: UsageLimitSettings) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = app_data_dir.join(SETTINGS_FILE_NAME);
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// A random slack in the 1–3 minute range.
pub fn random_slack_secs() -> i64 {
    let span = (RESUME_SLACK_MAX_SECS - RESUME_SLACK_MIN_SECS + 1) as u128;
    RESUME_SLACK_MIN_SECS + (uuid::Uuid::new_v4().as_u128() % span) as i64
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeAction {
    /// Nothing to do for this session (no limit, or not automatic).
    None,
    /// Waiting for `due`.
    Wait { due: DateTime<Utc> },
    /// Send the continuation now.
    Resume,
}

struct Watch {
    /// One stop: the turn and the reset it waits for. A new stop (or a reset
    /// learned late) is a new watch with a new slack.
    key: (TurnId, Option<String>),
    slack_secs: i64,
    due: Option<DateTime<Utc>>,
    failures: u32,
    retry_after: Option<DateTime<Utc>>,
}

/// The scheduler's decisions, without I/O or clock, so they can be tested.
#[derive(Default)]
pub struct ResumePlanner {
    watches: HashMap<String, Watch>,
}

impl ResumePlanner {
    /// Decides for one session given its current limit. `slack` gives the
    /// slack of a stop seen for the first time.
    pub fn update(
        &mut self,
        session_id: &str,
        limit: Option<&SessionUsageLimit>,
        now: DateTime<Utc>,
        slack: impl FnOnce() -> i64,
    ) -> ResumeAction {
        let Some(limit) = limit else {
            self.watches.remove(session_id);
            return ResumeAction::None;
        };
        let key = (limit.turn_id.clone(), limit.resets_at.clone());
        let watch = match self.watches.get_mut(session_id) {
            Some(watch) if watch.key == key => watch,
            _ => {
                self.watches.insert(
                    session_id.to_string(),
                    Watch {
                        key,
                        slack_secs: slack(),
                        due: None,
                        failures: 0,
                        retry_after: None,
                    },
                );
                self.watches.get_mut(session_id).expect("just inserted")
            }
        };
        watch.due = auto_resume_due_at(limit, watch.slack_secs, now);
        let Some(due) = watch.due else {
            return ResumeAction::None;
        };
        if watch.failures >= MAX_START_FAILURES {
            watch.due = None;
            return ResumeAction::None;
        }
        let ready = due.max(watch.retry_after.unwrap_or(due));
        if now >= ready {
            ResumeAction::Resume
        } else {
            ResumeAction::Wait { due: ready }
        }
    }

    /// The continuation could not start; try again later, a few times.
    pub fn failed(&mut self, session_id: &str, now: DateTime<Utc>) {
        if let Some(watch) = self.watches.get_mut(session_id) {
            watch.failures += 1;
            watch.retry_after = Some(now + chrono::Duration::seconds(RETRY_AFTER_FAILURE_SECS));
            if watch.failures >= MAX_START_FAILURES {
                watch.due = None;
            }
        }
    }

    /// Sessions that will resume by themselves, with when.
    pub fn waits(&self) -> Vec<(String, DateTime<Utc>)> {
        let mut waits: Vec<_> = self
            .watches
            .iter()
            .filter_map(|(session_id, watch)| watch.due.map(|due| (session_id.clone(), due)))
            .collect();
        waits.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        waits
    }
}

/// Runs the planner against the sessions' events for the life of the app.
pub struct UsageLimitScheduler {
    state: SessionCommandState,
    planner: Mutex<ResumePlanner>,
    watched: Mutex<HashSet<String>>,
    wake: tokio::sync::Notify,
}

impl UsageLimitScheduler {
    pub fn new(state: SessionCommandState) -> Arc<Self> {
        Arc::new(Self {
            state,
            planner: Mutex::new(ResumePlanner::default()),
            watched: Mutex::new(HashSet::new()),
            wake: tokio::sync::Notify::new(),
        })
    }

    /// Sessions that will resume by themselves, soonest first.
    pub fn waits(&self) -> Vec<(String, DateTime<Utc>)> {
        self.planner
            .lock()
            .map(|planner| planner.waits())
            .unwrap_or_default()
    }

    pub fn watch(&self, session_id: &str) {
        if let Ok(mut watched) = self.watched.lock() {
            watched.insert(session_id.to_string());
        }
        self.wake.notify_one();
    }

    /// Checks every watched session once; returns the next due time.
    pub async fn tick(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let sessions: Vec<String> = self
            .watched
            .lock()
            .map(|watched| watched.iter().cloned().collect())
            .unwrap_or_default();
        let mut next: Option<DateTime<Utc>> = None;
        for session_id in sessions {
            let limit = match self
                .state
                .session_usage_limit(&SessionId(session_id.clone()))
            {
                Ok(limit) => limit,
                Err(error) => {
                    eprintln!("[DCC][usage-limit] {session_id}: {error}");
                    continue;
                }
            };
            if limit.is_none() {
                if let Ok(mut watched) = self.watched.lock() {
                    watched.remove(&session_id);
                }
            }
            let action = match self.planner.lock() {
                Ok(mut planner) => {
                    planner.update(&session_id, limit.as_ref(), now, random_slack_secs)
                }
                Err(_) => continue,
            };
            match action {
                ResumeAction::None => {}
                ResumeAction::Wait { due } => {
                    next = Some(next.map_or(due, |current| current.min(due)));
                }
                ResumeAction::Resume => {
                    match self
                        .state
                        .resume_after_usage_limit(&SessionId(session_id.clone()), true)
                        .await
                    {
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("[DCC][usage-limit] resume of {session_id} failed: {error}");
                            if let Ok(mut planner) = self.planner.lock() {
                                planner.failed(&session_id, Utc::now());
                            }
                        }
                    }
                }
            }
        }
        next
    }

    /// Reconciles the sessions that ever stopped on a limit, then checks
    /// them every `CHECK_EVERY` (or sooner, when one is due or a new limit
    /// arrives).
    pub async fn run(self: Arc<Self>) {
        match self.state.sessions_with_usage_limit() {
            Ok(sessions) => {
                if let Ok(mut watched) = self.watched.lock() {
                    watched.extend(sessions.into_iter().map(|id| id.0));
                }
            }
            Err(error) => eprintln!("[DCC][usage-limit] startup reconcile failed: {error}"),
        }
        loop {
            let next = self.tick(Utc::now()).await;
            let mut wait = CHECK_EVERY;
            if let Some(next) = next {
                let until = (next - Utc::now()).to_std().unwrap_or(Duration::ZERO);
                wait = wait.min(until.max(Duration::from_millis(250)));
            }
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = self.wake.notified() => {}
            }
        }
    }

    /// The bus to register on the process runtime (it keeps a weak
    /// reference; the caller holds the returned `Arc`).
    pub fn event_bus(self: &Arc<Self>) -> Arc<dyn EventBus> {
        Arc::new(SchedulerEvents(Arc::downgrade(self)))
    }
}

struct SchedulerEvents(Weak<UsageLimitScheduler>);

#[async_trait]
impl EventBus for SchedulerEvents {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        let Some(scheduler) = self.0.upgrade() else {
            return Ok(());
        };
        match &event {
            // The limit is recorded just before the turn's abort, and it only
            // stands once the abort is there: a tick in between would see no
            // limit and drop the session, so the abort is watched too.
            CoreEvent::SessionUsageLimitReached { session_id, .. }
            | CoreEvent::SessionUsageLimitAutoResumeChanged { session_id, .. }
            | CoreEvent::SessionTurnAborted { session_id, .. } => scheduler.watch(session_id),
            // A new turn clears the limit; re-check only sessions followed.
            CoreEvent::SessionTurnStarted { session_id, .. } => {
                let watched = scheduler
                    .watched
                    .lock()
                    .map(|watched| watched.contains(session_id))
                    .unwrap_or(false);
                if watched {
                    scheduler.watch(session_id);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn limit(resets_at: Option<&str>, auto_resume: bool, used: u32) -> SessionUsageLimit {
        SessionUsageLimit {
            turn_id: TurnId("t1".to_string()),
            hit_at: "2026-10-12T01:30:00Z".to_string(),
            resets_at: resets_at.map(str::to_string),
            auto_resume,
            auto_resumes_used: used,
        }
    }

    #[test]
    fn slack_is_one_to_three_minutes() {
        for _ in 0..500 {
            let slack = random_slack_secs();
            assert!((60..=180).contains(&slack), "{slack}");
        }
    }

    #[test]
    fn resumes_once_the_reset_plus_slack_passes() {
        let mut planner = ResumePlanner::default();
        let stop = limit(Some("2026-10-12T03:40:00Z"), true, 0);
        let action = planner.update("s", Some(&stop), time("2026-10-12T02:00:00Z"), || 120);
        assert_eq!(
            action,
            ResumeAction::Wait {
                due: time("2026-10-12T03:42:00Z")
            }
        );
        assert_eq!(
            planner.waits(),
            vec![("s".to_string(), time("2026-10-12T03:42:00Z"))]
        );
        // The slack is chosen once per stop, not on every tick.
        let action = planner.update("s", Some(&stop), time("2026-10-12T03:41:00Z"), || 60);
        assert_eq!(
            action,
            ResumeAction::Wait {
                due: time("2026-10-12T03:42:00Z")
            }
        );
        let action = planner.update("s", Some(&stop), time("2026-10-12T03:42:00Z"), || 60);
        assert_eq!(action, ResumeAction::Resume);
    }

    #[test]
    fn after_a_restart_a_passed_reset_resumes_at_once() {
        // A fresh planner (the app just started) sees a reset hours ago.
        let mut planner = ResumePlanner::default();
        let stop = limit(Some("2026-10-12T03:40:00Z"), true, 0);
        let action = planner.update("s", Some(&stop), time("2026-10-12T08:00:00Z"), || 90);
        assert_eq!(action, ResumeAction::Resume);
    }

    #[test]
    fn nothing_resumes_without_the_choice_a_reset_or_attempts_left() {
        let mut planner = ResumePlanner::default();
        let now = time("2026-10-12T05:00:00Z");
        for stop in [
            limit(Some("2026-10-12T03:40:00Z"), false, 0),
            limit(None, true, 0),
            limit(Some("2026-10-12T03:40:00Z"), true, 3),
        ] {
            assert_eq!(
                planner.update("s", Some(&stop), now, || 90),
                ResumeAction::None
            );
        }
        assert!(planner.waits().is_empty());
        assert_eq!(planner.update("s", None, now, || 90), ResumeAction::None);
    }

    #[test]
    fn a_failed_start_retries_later_then_gives_up() {
        let mut planner = ResumePlanner::default();
        let stop = limit(Some("2026-10-12T03:40:00Z"), true, 0);
        let now = time("2026-10-12T04:00:00Z");
        assert_eq!(
            planner.update("s", Some(&stop), now, || 90),
            ResumeAction::Resume
        );
        planner.failed("s", now);
        assert_eq!(
            planner.update("s", Some(&stop), now, || 90),
            ResumeAction::Wait {
                due: time("2026-10-12T04:05:00Z")
            }
        );
        planner.failed("s", now);
        planner.failed("s", now);
        assert_eq!(
            planner.update("s", Some(&stop), time("2026-10-12T05:00:00Z"), || 90),
            ResumeAction::None
        );
        assert!(planner.waits().is_empty());
    }

    #[test]
    fn a_late_reset_is_a_new_stop() {
        let mut planner = ResumePlanner::default();
        let now = time("2026-10-12T02:00:00Z");
        assert_eq!(
            planner.update("s", Some(&limit(None, true, 0)), now, || 90),
            ResumeAction::None
        );
        let action = planner.update(
            "s",
            Some(&limit(Some("2026-10-12T03:40:00Z"), true, 0)),
            now,
            || 60,
        );
        assert_eq!(
            action,
            ResumeAction::Wait {
                due: time("2026-10-12T03:41:00Z")
            }
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_limit_seen_before_its_abort_is_still_followed() {
        use dcc_core::domain::session::{Session, SessionEventKind, SessionState};
        use dcc_core::domain::{project::ProjectId, workspace::WorkspaceId};
        use dcc_core::ports::SessionRepo;
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let state =
            SessionCommandState::new_headless(root.join("state.sqlite"), root.join("app-data"));
        let session = Session {
            id: SessionId("race".to_string()),
            project_id: ProjectId("p".to_string()),
            workspace_id: WorkspaceId("w".to_string()),
            additional_workspace_ids: Vec::new(),
            provider_id: "codex".to_string(),
            model: None,
            provider_runtime: None,
            working_directory_override: None,
            state: SessionState::Active,
            created_at: "2026-10-12T00:00:00Z".to_string(),
            updated_at: "2026-10-12T00:00:00Z".to_string(),
        };
        SessionRepo::save_session(&state, &session).await.unwrap();
        let turn = TurnId("t1".to_string());
        state
            .append_session_event(
                &session.id,
                SessionEventKind::TurnStarted {
                    turn_id: turn.clone(),
                    prompt: "p".to_string(),
                    plan_mode: None,
                    model: None,
                    evidence: None,
                    retry_of_turn_id: None,
                    approval_policy: None,
                },
            )
            .await
            .unwrap();
        let resets_at = (Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        state
            .append_session_event(
                &session.id,
                SessionEventKind::UsageLimitReached {
                    turn_id: turn.clone(),
                    resets_at: Some(resets_at.clone()),
                    auto_resume: true,
                    paused: false,
                },
            )
            .await
            .unwrap();
        let scheduler = UsageLimitScheduler::new(state.clone());
        let bus = scheduler.event_bus();
        bus.publish(CoreEvent::SessionUsageLimitReached {
            session_id: "race".to_string(),
            turn_id: "t1".to_string(),
            resets_at: Some(resets_at),
            auto_resume: true,
            paused: false,
        })
        .await
        .unwrap();
        // The tick lands before the abort: no limit stands yet.
        scheduler.tick(Utc::now()).await;
        assert!(scheduler.waits().is_empty());
        state
            .append_session_event(
                &session.id,
                SessionEventKind::TurnAborted {
                    turn_id: turn,
                    reason: Some("limit".to_string()),
                },
            )
            .await
            .unwrap();
        bus.publish(CoreEvent::SessionTurnAborted {
            session_id: "race".to_string(),
            turn_id: "t1".to_string(),
            reason: Some("limit".to_string()),
        })
        .await
        .unwrap();
        scheduler.tick(Utc::now()).await;
        assert_eq!(scheduler.waits().len(), 1);
    }

    #[test]
    fn settings_default_off_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("dcc-usage-limit-{}", uuid::Uuid::new_v4()));
        assert!(!load_settings(&dir).auto_resume);
        save_settings(&dir, UsageLimitSettings { auto_resume: true }).unwrap();
        assert!(load_settings(&dir).auto_resume);
        let _ = std::fs::remove_dir_all(dir);
    }
}
