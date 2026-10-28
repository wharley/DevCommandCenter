//! Keeps the Mac awake while DCC has work to do, so agents keep running and
//! the paired phone keeps its connection when the person walks away.
//!
//! One global preference (`keep-awake-settings.json` in the app data dir):
//! `Off` (default), `Always` (while DCC is open) and `DuringAgents`.
//!
//! "During agents" follows the session events, not hooks in the session
//! state: this module registers its own `EventBus` on the process runtime and
//! keeps the set of turns in flight (`SessionTurnStarted` in,
//! `SessionTurnCompleted` / `SessionTurnAborted` out — a provider failure
//! arrives as an abort). A turn waiting for an approval or an answer is still
//! in flight: the person may answer it from the phone. Every
//! `RECONCILE_EVERY` the set is checked against the provider bindings
//! (`SessionCommandState::active_turns`), so a lost terminal event cannot keep
//! the Mac awake forever, and a missed start cannot let it sleep mid-turn.
//! After a restart the set starts empty: provider processes do not survive
//! the app, and the first reconcile runs at startup.
//!
//! When the last turn ends the hold lingers for `HAND_OFF_GRACE`. A queued
//! follow-up (sessions without an objective, or with an active one, dispatch
//! it right after the completion) starts within that window, and releasing in
//! between would let a Mac that has been idle for hours sleep at once,
//! before the next turn starts. A queue left behind by a failed turn does not
//! dispatch by itself, so it does not hold the Mac.
//!
//! A task stopped by its plan's usage limit that will continue by itself
//! (`dcc_tauri::usage_limit_resume`) holds the Mac too, until the scheduler
//! resumes it (reset plus slack): otherwise the Mac sleeps before the reset
//! and the resume never runs. A limit that waits for the person does not.
//!
//! macOS: an IOKit `PreventUserIdleSystemSleep` assertion — the system stays
//! awake, the display may still sleep. It does not survive closing the lid:
//! macOS sleeps then, on battery or plugged in, except in Apple's
//! closed-display mode (power, external display, keyboard and mouse). The
//! kernel drops the assertion if DCC exits or crashes. Elsewhere this is a
//! documented no-op and the setting is hidden (`supported() == false`).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dcc_core::ports::events::CoreEvent;
use dcc_core::ports::EventBus;
use serde::{Deserialize, Serialize};

const SETTINGS_FILE_NAME: &str = "keep-awake-settings.json";
/// How long the hold outlives the last turn (queued follow-up hand-off).
pub const HAND_OFF_GRACE: Duration = Duration::from_secs(120);
/// How often the turn set is checked against the provider bindings.
pub const RECONCILE_EVERY: Duration = Duration::from_secs(30);
/// A turn the events reported is only dropped by a reconcile once it is this
/// old: `SessionTurnStarted` is published just before the binding records it.
const RECONCILE_MIN_AGE: Duration = Duration::from_secs(20);
/// How long an ended turn is remembered, so a reconcile that read the
/// bindings just before the end does not bring it back.
const ENDED_MEMORY: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepAwakeMode {
    #[default]
    Off,
    Always,
    DuringAgents,
}

/// Why the Mac is being kept awake.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HoldReason {
    AppOpen,
    AgentsWorking {
        count: usize,
    },
    /// The last turn just ended; waiting out `HAND_OFF_GRACE`.
    AgentJustFinished,
    /// A task resumes by itself after its usage limit resets (RFC 3339).
    WaitingForLimit {
        until: String,
    },
}

impl HoldReason {
    /// Text for native surfaces (menu bar tooltip, `pmset -g assertions`).
    pub fn label(&self) -> String {
        match self {
            Self::AppOpen => "DCC aberto".to_string(),
            Self::AgentsWorking { count: 1 } => "1 agente trabalhando".to_string(),
            Self::AgentsWorking { count } => format!("{count} agentes trabalhando"),
            Self::AgentJustFinished => "agente acabou de terminar".to_string(),
            Self::WaitingForLimit { until } => {
                match chrono::DateTime::parse_from_rfc3339(until) {
                    Ok(until) => format!(
                        "aguardando limite até {}",
                        until.with_timezone(&chrono::Local).format("%H:%M")
                    ),
                    Err(_) => "aguardando limite".to_string(),
                }
            }
        }
    }
}

struct TrackedTurn {
    turn_id: String,
    since: Instant,
}

/// Turns in flight, keyed by session (a session runs one turn at a time).
#[derive(Default)]
pub struct ActiveTurns {
    turns: HashMap<String, TrackedTurn>,
    ended: HashMap<(String, String), Instant>,
    last_ended: Option<Instant>,
}

impl ActiveTurns {
    pub fn started(&mut self, session_id: &str, turn_id: &str, now: Instant) {
        self.turns.insert(
            session_id.to_string(),
            TrackedTurn {
                turn_id: turn_id.to_string(),
                since: now,
            },
        );
    }

    /// Ends `turn_id` if it is still the session's current turn; a late end
    /// of an older turn leaves the current one alone.
    pub fn ended(&mut self, session_id: &str, turn_id: &str, now: Instant) {
        self.ended
            .insert((session_id.to_string(), turn_id.to_string()), now);
        if self
            .turns
            .get(session_id)
            .is_some_and(|turn| turn.turn_id == turn_id)
        {
            self.turns.remove(session_id);
            self.last_ended = Some(now);
        }
    }

    pub fn session_closed(&mut self, session_id: &str, now: Instant) {
        if let Some(turn) = self.turns.remove(session_id) {
            self.ended
                .insert((session_id.to_string(), turn.turn_id), now);
            self.last_ended = Some(now);
        }
    }

    /// Aligns the set with the provider bindings (`truth`).
    pub fn reconcile(&mut self, truth: &[(String, String)], now: Instant) {
        self.ended
            .retain(|_, at| now.saturating_duration_since(*at) < ENDED_MEMORY);
        let mut dropped = false;
        self.turns.retain(|session_id, turn| {
            let alive = truth
                .iter()
                .any(|(id, turn_id)| id == session_id && *turn_id == turn.turn_id);
            let keep = alive || now.saturating_duration_since(turn.since) < RECONCILE_MIN_AGE;
            dropped |= !keep;
            keep
        });
        if dropped {
            self.last_ended = Some(now);
        }
        for (session_id, turn_id) in truth {
            let known = self
                .turns
                .get(session_id)
                .is_some_and(|turn| &turn.turn_id == turn_id);
            let ended = self
                .ended
                .contains_key(&(session_id.clone(), turn_id.clone()));
            if !known && !ended {
                self.started(session_id, turn_id, now);
            }
        }
    }

    pub fn count(&self) -> usize {
        self.turns.len()
    }

    /// When the hand-off grace ends, if no turn is in flight.
    fn hand_off_ends(&self, grace: Duration) -> Option<Instant> {
        if self.turns.is_empty() {
            self.last_ended.map(|at| at + grace)
        } else {
            None
        }
    }

    fn ended_within(&self, grace: Duration, now: Instant) -> bool {
        self.last_ended
            .is_some_and(|at| now.saturating_duration_since(at) < grace)
    }
}

/// What the preference asks for right now. `limit_until` is the soonest
/// automatic resume after a usage limit, if a task waits for one: in
/// `DuringAgents` it holds the Mac like a turn in flight, checked after the
/// active turns.
pub fn decide(
    mode: KeepAwakeMode,
    turns: &ActiveTurns,
    limit_until: Option<&chrono::DateTime<chrono::Utc>>,
    grace: Duration,
    now: Instant,
) -> Option<HoldReason> {
    match mode {
        KeepAwakeMode::Off => None,
        KeepAwakeMode::Always => Some(HoldReason::AppOpen),
        KeepAwakeMode::DuringAgents => match (turns.count(), limit_until) {
            (0, Some(until)) => Some(HoldReason::WaitingForLimit {
                until: until.to_rfc3339(),
            }),
            (0, None) if turns.ended_within(grace, now) => Some(HoldReason::AgentJustFinished),
            (0, None) => None,
            (count, _) => Some(HoldReason::AgentsWorking { count }),
        },
    }
}

/// The OS layer: one sleep-preventing assertion at a time.
pub trait PowerAssertions: Send + Sync {
    fn supported(&self) -> bool;
    fn acquire(&self, name: &str) -> Result<u32, String>;
    fn release(&self, id: u32);
}

/// Where "During agents" reads the turns really in flight.
#[async_trait]
pub trait TurnSource: Send + Sync {
    async fn active_turns(&self) -> Result<Vec<(String, String)>, String>;

    /// When the tasks waiting on a usage limit resume by themselves.
    async fn limit_waits(&self) -> Vec<chrono::DateTime<chrono::Utc>> {
        Vec::new()
    }
}

#[async_trait]
impl TurnSource for dcc_tauri::state::SessionCommandState {
    async fn active_turns(&self) -> Result<Vec<(String, String)>, String> {
        dcc_tauri::state::SessionCommandState::active_turns(self)
            .await
            .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
pub struct SystemPower;

#[cfg(target_os = "macos")]
mod iokit {
    use std::ffi::{c_char, c_void};

    pub type CFStringRef = *const c_void;
    pub const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// `kIOPMAssertionLevelOn`.
    pub const ASSERTION_LEVEL_ON: u32 = 255;
    /// `kIOPMAssertionTypePreventUserIdleSystemSleep`.
    pub const PREVENT_USER_IDLE_SYSTEM_SLEEP: &std::ffi::CStr = c"PreventUserIdleSystemSleep";
    pub const IO_RETURN_SUCCESS: i32 = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn CFStringCreateWithCString(
            allocator: *const c_void,
            text: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        pub fn CFRelease(value: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        pub fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            level: u32,
            name: CFStringRef,
            id: *mut u32,
        ) -> i32;
        pub fn IOPMAssertionRelease(id: u32) -> i32;
    }

    pub struct CfString(pub CFStringRef);

    impl CfString {
        pub fn new(text: &std::ffi::CStr) -> Option<Self> {
            let value = unsafe {
                CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), CF_STRING_ENCODING_UTF8)
            };
            (!value.is_null()).then_some(Self(value))
        }
    }

    impl Drop for CfString {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }
}

#[cfg(target_os = "macos")]
impl PowerAssertions for SystemPower {
    fn supported(&self) -> bool {
        true
    }

    fn acquire(&self, name: &str) -> Result<u32, String> {
        let name = std::ffi::CString::new(name.replace('\0', "")).map_err(|e| e.to_string())?;
        let kind = iokit::CfString::new(iokit::PREVENT_USER_IDLE_SYSTEM_SLEEP)
            .ok_or("assertion type unavailable")?;
        let name = iokit::CfString::new(&name).ok_or("assertion name unavailable")?;
        let mut id = 0u32;
        let status = unsafe {
            iokit::IOPMAssertionCreateWithName(kind.0, iokit::ASSERTION_LEVEL_ON, name.0, &mut id)
        };
        if status == iokit::IO_RETURN_SUCCESS {
            Ok(id)
        } else {
            Err(format!("IOPMAssertionCreateWithName failed: {status:#x}"))
        }
    }

    fn release(&self, id: u32) {
        let status = unsafe { iokit::IOPMAssertionRelease(id) };
        if status != iokit::IO_RETURN_SUCCESS {
            eprintln!("[DCC][keep-awake] IOPMAssertionRelease failed: {status:#x}");
        }
    }
}

/// Non-macOS: nothing to hold; the setting is hidden.
pub struct NoPower;

impl PowerAssertions for NoPower {
    fn supported(&self) -> bool {
        false
    }
    fn acquire(&self, _name: &str) -> Result<u32, String> {
        Err("keeping the computer awake is only available on macOS".to_string())
    }
    fn release(&self, _id: u32) {}
}

pub fn system_power() -> Arc<dyn PowerAssertions> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(SystemPower)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(NoPower)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    #[serde(default)]
    mode: KeepAwakeMode,
}

pub fn load_mode(app_data_dir: &Path) -> KeepAwakeMode {
    std::fs::read(app_data_dir.join(SETTINGS_FILE_NAME))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PersistedSettings>(&bytes).ok())
        .map(|settings| settings.mode)
        .unwrap_or_default()
}

fn save_mode(app_data_dir: &Path, mode: KeepAwakeMode) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = app_data_dir.join(SETTINGS_FILE_NAME);
    let temp = path.with_extension("json.tmp");
    let bytes =
        serde_json::to_vec_pretty(&PersistedSettings { mode }).map_err(|e| e.to_string())?;
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepAwakeHold {
    pub reason: HoldReason,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepAwakeStatus {
    pub supported: bool,
    pub mode: KeepAwakeMode,
    pub active: Option<KeepAwakeHold>,
}

type ChangeListener = Box<dyn Fn(Option<&HoldReason>) + Send + Sync>;

struct Inner {
    mode: KeepAwakeMode,
    turns: ActiveTurns,
    limit_until: Option<chrono::DateTime<chrono::Utc>>,
    held: Option<(u32, HoldReason)>,
}

pub struct KeepAwake {
    inner: Mutex<Inner>,
    power: Arc<dyn PowerAssertions>,
    settings_dir: Option<PathBuf>,
    grace: Duration,
    listener: Mutex<Option<ChangeListener>>,
    wake: tokio::sync::Notify,
}

impl KeepAwake {
    pub fn new(
        power: Arc<dyn PowerAssertions>,
        settings_dir: Option<PathBuf>,
        mode: KeepAwakeMode,
        grace: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                mode,
                turns: ActiveTurns::default(),
                limit_until: None,
                held: None,
            }),
            power,
            settings_dir,
            grace,
            listener: Mutex::new(None),
            wake: tokio::sync::Notify::new(),
        })
    }

    /// Called with the new reason (or `None`) whenever the hold changes.
    pub fn set_listener(&self, listener: ChangeListener) {
        if let Ok(mut slot) = self.listener.lock() {
            *slot = Some(listener);
        }
    }

    pub fn status(&self) -> KeepAwakeStatus {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        KeepAwakeStatus {
            supported: self.power.supported(),
            mode: inner.mode,
            active: inner.held.as_ref().map(|(_, reason)| KeepAwakeHold {
                label: reason.label(),
                reason: reason.clone(),
            }),
        }
    }

    pub fn set_mode(&self, mode: KeepAwakeMode) -> Result<KeepAwakeStatus, String> {
        if let Some(dir) = &self.settings_dir {
            save_mode(dir, mode)?;
        }
        self.update(Instant::now(), |inner| inner.mode = mode);
        Ok(self.status())
    }

    pub fn turn_started(&self, session_id: &str, turn_id: &str, now: Instant) {
        self.update(now, |inner| inner.turns.started(session_id, turn_id, now));
    }

    pub fn turn_ended(&self, session_id: &str, turn_id: &str, now: Instant) {
        self.update(now, |inner| inner.turns.ended(session_id, turn_id, now));
    }

    pub fn session_closed(&self, session_id: &str, now: Instant) {
        self.update(now, |inner| inner.turns.session_closed(session_id, now));
    }

    pub fn reconcile(&self, truth: &[(String, String)], now: Instant) {
        self.update(now, |inner| inner.turns.reconcile(truth, now));
    }

    /// The automatic resumes tasks wait for; the soonest one is what holds.
    pub fn set_limit_waits(&self, waits: &[chrono::DateTime<chrono::Utc>], now: Instant) {
        let until = waits.iter().min().copied();
        self.update(now, |inner| inner.limit_until = until);
    }

    /// Re-evaluates without new input (grace expiry).
    pub fn tick(&self, now: Instant) {
        self.update(now, |_| {});
    }

    /// Applies `change`, then takes or lets go of the assertion to match.
    /// A new assertion is taken before the old one is released, so a change
    /// of reason never leaves a gap.
    fn update(&self, now: Instant, change: impl FnOnce(&mut Inner)) {
        let changed = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut inner);
            let wanted = if self.power.supported() {
                decide(
                    inner.mode,
                    &inner.turns,
                    inner.limit_until.as_ref(),
                    self.grace,
                    now,
                )
            } else {
                None
            };
            let current = inner.held.as_ref().map(|(_, reason)| reason.clone());
            if wanted == current {
                None
            } else {
                match wanted {
                    Some(reason) => {
                        match self.power.acquire(&assertion_name(&reason)) {
                            Ok(id) => {
                                if let Some((old, _)) = inner.held.replace((id, reason.clone())) {
                                    self.power.release(old);
                                }
                                Some(Some(reason))
                            }
                            Err(error) => {
                                // Keep whatever is held; the next event or
                                // tick tries again.
                                eprintln!("[DCC][keep-awake] {error}");
                                None
                            }
                        }
                    }
                    None => {
                        if let Some((old, _)) = inner.held.take() {
                            self.power.release(old);
                        }
                        Some(None)
                    }
                }
            }
        };
        if let Some(reason) = changed {
            self.wake.notify_one();
            if let Ok(listener) = self.listener.lock() {
                if let Some(listener) = listener.as_ref() {
                    listener(reason.as_ref());
                }
            }
        }
    }

    /// When the hand-off grace ends, while that is what holds the Mac.
    fn hand_off_deadline(&self) -> Option<Instant> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match inner.held {
            Some((_, HoldReason::AgentJustFinished)) => inner.turns.hand_off_ends(self.grace),
            _ => None,
        }
    }

    /// Reconciles with `source` every `RECONCILE_EVERY` and wakes when the
    /// hand-off grace ends, so the Mac is let go on time. Runs for the life
    /// of the app.
    pub async fn run(self: Arc<Self>, source: Arc<dyn TurnSource>) {
        let mut next_reconcile = Instant::now();
        loop {
            let now = Instant::now();
            if now >= next_reconcile {
                let waits = source.limit_waits().await;
                self.set_limit_waits(&waits, Instant::now());
                match source.active_turns().await {
                    Ok(truth) => self.reconcile(&truth, Instant::now()),
                    Err(error) => {
                        eprintln!("[DCC][keep-awake] reconcile skipped: {error}");
                        self.tick(Instant::now());
                    }
                }
                next_reconcile = Instant::now() + RECONCILE_EVERY;
            } else {
                self.tick(now);
            }
            let wake_at = self
                .hand_off_deadline()
                .map_or(next_reconcile, |deadline| deadline.min(next_reconcile));
            tokio::select! {
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(wake_at)) => {}
                // A change (e.g. the last turn ended) may set an earlier deadline.
                _ = self.wake.notified() => {}
            }
        }
    }

    /// The bus to register on the process runtime. The runtime keeps only a
    /// weak reference: the caller holds the returned `Arc`.
    pub fn event_bus(self: &Arc<Self>) -> Arc<dyn EventBus> {
        Arc::new(KeepAwakeEvents(Arc::downgrade(self)))
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        let inner = self.inner.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Some((id, _)) = inner.held.take() {
            self.power.release(id);
        }
    }
}

fn assertion_name(reason: &HoldReason) -> String {
    format!("Dev Command Center: Mac acordado ({})", reason.label())
}

struct KeepAwakeEvents(Weak<KeepAwake>);

#[async_trait]
impl EventBus for KeepAwakeEvents {
    async fn publish(&self, event: CoreEvent) -> dcc_core::Result<()> {
        let Some(controller) = self.0.upgrade() else {
            return Ok(());
        };
        let now = Instant::now();
        match &event {
            CoreEvent::SessionTurnStarted {
                session_id,
                turn_id,
                ..
            } => controller.turn_started(session_id, turn_id, now),
            CoreEvent::SessionTurnCompleted {
                session_id,
                turn_id,
            }
            | CoreEvent::SessionTurnAborted {
                session_id,
                turn_id,
                ..
            } => controller.turn_ended(session_id, turn_id, now),
            CoreEvent::SessionCompleted { session_id, .. }
            | CoreEvent::SessionAborted { session_id, .. } => {
                controller.session_closed(session_id, now)
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakePower {
        log: Mutex<Vec<String>>,
        next: Mutex<u32>,
        held: Mutex<Vec<u32>>,
        fail: std::sync::atomic::AtomicBool,
    }

    impl PowerAssertions for FakePower {
        fn supported(&self) -> bool {
            true
        }
        fn acquire(&self, name: &str) -> Result<u32, String> {
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("denied".into());
            }
            let mut next = self.next.lock().unwrap();
            *next += 1;
            self.held.lock().unwrap().push(*next);
            self.log
                .lock()
                .unwrap()
                .push(format!("acquire {} {name}", *next));
            Ok(*next)
        }
        fn release(&self, id: u32) {
            self.held.lock().unwrap().retain(|held| *held != id);
            self.log.lock().unwrap().push(format!("release {id}"));
        }
    }

    impl FakePower {
        fn held(&self) -> Vec<u32> {
            self.held.lock().unwrap().clone()
        }
        fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    const GRACE: Duration = Duration::from_secs(120);

    fn controller(mode: KeepAwakeMode) -> (Arc<FakePower>, Arc<KeepAwake>) {
        let power = Arc::new(FakePower::default());
        let keep = KeepAwake::new(power.clone(), None, mode, GRACE);
        (power, keep)
    }

    fn label(keep: &KeepAwake) -> Option<String> {
        keep.status().active.map(|hold| hold.label)
    }

    #[test]
    fn off_never_holds_and_always_holds_without_agents() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::Off);
        keep.turn_started("s", "t", t0);
        assert!(power.held().is_empty());
        assert_eq!(keep.status().active, None);

        keep.set_mode(KeepAwakeMode::Always).unwrap();
        assert_eq!(power.held().len(), 1);
        assert_eq!(label(&keep).as_deref(), Some("DCC aberto"));
        keep.turn_ended("s", "t", t0);
        assert_eq!(power.held().len(), 1, "Always ignores the agents");

        keep.set_mode(KeepAwakeMode::Off).unwrap();
        assert!(power.held().is_empty());
    }

    #[test]
    fn during_agents_follows_turns_and_lingers_for_the_hand_off() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        assert!(power.held().is_empty());

        keep.turn_started("a", "a1", t0);
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
        keep.turn_started("b", "b1", t0);
        assert_eq!(label(&keep).as_deref(), Some("2 agentes trabalhando"));
        assert_eq!(power.held().len(), 1, "one assertion at a time");

        keep.turn_ended("a", "a1", t0);
        keep.turn_ended("b", "b1", t0 + Duration::from_secs(1));
        assert_eq!(label(&keep).as_deref(), Some("agente acabou de terminar"));

        // A queued follow-up starts inside the grace: no gap.
        keep.turn_started("b", "b2", t0 + Duration::from_secs(2));
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
        keep.turn_ended("b", "b2", t0 + Duration::from_secs(10));
        keep.tick(t0 + Duration::from_secs(10) + GRACE);
        assert!(power.held().is_empty());
        assert_eq!(keep.status().active, None);
    }

    #[test]
    fn a_late_end_of_an_older_turn_keeps_the_current_one() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        keep.turn_started("s", "old", t0);
        keep.turn_started("s", "new", t0);
        keep.turn_ended("s", "old", t0);
        keep.tick(t0 + GRACE * 2);
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
        assert_eq!(power.held().len(), 1);
        keep.session_closed("s", t0 + GRACE * 2);
        keep.tick(t0 + GRACE * 4);
        assert!(power.held().is_empty());
    }

    #[test]
    fn reconcile_drops_turns_that_never_closed_and_adopts_missed_starts() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        keep.turn_started("lost", "t", t0);
        // Too young to drop: the binding may not have recorded it yet.
        keep.reconcile(&[], t0 + Duration::from_secs(5));
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
        // The terminal event never came.
        let t1 = t0 + Duration::from_secs(60);
        keep.reconcile(&[], t1);
        assert_eq!(label(&keep).as_deref(), Some("agente acabou de terminar"));
        keep.reconcile(&[], t1 + GRACE);
        assert!(power.held().is_empty());

        // A start the bus missed (registered late, lagged).
        let t2 = t1 + GRACE * 2;
        keep.reconcile(&[("missed".into(), "m".into())], t2);
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
    }

    #[test]
    fn reconcile_does_not_revive_a_turn_that_just_ended() {
        let t0 = Instant::now();
        let (_power, keep) = controller(KeepAwakeMode::DuringAgents);
        keep.turn_started("s", "t", t0);
        // Bindings read just before the end; applied just after.
        let stale = vec![("s".to_string(), "t".to_string())];
        keep.turn_ended("s", "t", t0 + Duration::from_secs(1));
        keep.reconcile(&stale, t0 + Duration::from_secs(1));
        keep.tick(t0 + GRACE * 2);
        assert_eq!(keep.status().active, None);
    }

    #[test]
    fn a_change_of_reason_takes_the_new_assertion_before_releasing_the_old() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        keep.turn_started("a", "1", t0);
        keep.turn_started("b", "1", t0);
        assert_eq!(
            power.log(),
            vec![
                "acquire 1 Dev Command Center: Mac acordado (1 agente trabalhando)",
                "acquire 2 Dev Command Center: Mac acordado (2 agentes trabalhando)",
                "release 1",
            ]
        );
        drop(keep);
        assert!(power.held().is_empty(), "dropping the controller releases");
    }

    #[test]
    fn a_failed_acquire_is_retried_on_the_next_tick() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::Always);
        power.fail.store(true, std::sync::atomic::Ordering::SeqCst);
        keep.tick(t0);
        assert_eq!(keep.status().active, None);
        power.fail.store(false, std::sync::atomic::Ordering::SeqCst);
        keep.tick(t0);
        assert_eq!(power.held().len(), 1);
    }

    #[test]
    fn listener_hears_each_change_once() {
        let t0 = Instant::now();
        let (_power, keep) = controller(KeepAwakeMode::DuringAgents);
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = heard.clone();
        keep.set_listener(Box::new(move |reason| {
            sink.lock().unwrap().push(reason.map(HoldReason::label));
        }));
        keep.turn_started("s", "t", t0);
        keep.turn_started("s", "t", t0);
        keep.turn_ended("s", "t", t0);
        keep.tick(t0 + GRACE);
        assert_eq!(
            *heard.lock().unwrap(),
            vec![
                Some("1 agente trabalhando".to_string()),
                Some("agente acabou de terminar".to_string()),
                None,
            ]
        );
    }

    #[test]
    fn a_task_waiting_for_its_limit_holds_the_mac_until_it_resumes() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        keep.turn_started("s", "t", t0);
        keep.turn_ended("s", "t", t0);
        let until = chrono::DateTime::parse_from_rfc3339("2026-10-12T06:41:30Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        keep.set_limit_waits(&[until], t0);
        // Long after the hand-off grace, the wait still holds.
        keep.tick(t0 + GRACE * 10);
        assert_eq!(power.held().len(), 1);
        assert!(matches!(
            keep.status().active.map(|hold| hold.reason),
            Some(HoldReason::WaitingForLimit { .. })
        ));
        assert!(label(&keep).unwrap().starts_with("aguardando limite até "));
        // The resume starts a turn: agents working wins.
        keep.turn_started("s", "resume", t0 + GRACE * 10);
        assert_eq!(label(&keep).as_deref(), Some("1 agente trabalhando"));
        keep.set_limit_waits(&[], t0 + GRACE * 10);
        keep.turn_ended("s", "resume", t0 + GRACE * 11);
        keep.tick(t0 + GRACE * 13);
        assert!(power.held().is_empty());
    }

    #[test]
    fn a_limit_wait_does_not_hold_when_off() {
        let t0 = Instant::now();
        let (power, keep) = controller(KeepAwakeMode::Off);
        keep.set_limit_waits(&[chrono::Utc::now()], t0);
        assert!(power.held().is_empty());
    }

    #[tokio::test]
    async fn run_reads_the_limit_waits_from_the_source() {
        struct Waiting;
        #[async_trait]
        impl TurnSource for Waiting {
            async fn active_turns(&self) -> Result<Vec<(String, String)>, String> {
                Ok(Vec::new())
            }
            async fn limit_waits(&self) -> Vec<chrono::DateTime<chrono::Utc>> {
                vec![chrono::Utc::now() + chrono::Duration::minutes(30)]
            }
        }
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        tokio::spawn(keep.clone().run(Arc::new(Waiting)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(power.held().len(), 1);
    }

    #[test]
    fn unsupported_platforms_never_hold() {
        let keep = KeepAwake::new(Arc::new(NoPower), None, KeepAwakeMode::Always, GRACE);
        keep.tick(Instant::now());
        let status = keep.status();
        assert!(!status.supported);
        assert_eq!(status.active, None);
    }

    #[tokio::test]
    async fn the_event_bus_feeds_the_counter() {
        let (power, keep) = controller(KeepAwakeMode::DuringAgents);
        let bus = keep.event_bus();
        bus.publish(CoreEvent::SessionTurnStarted {
            session_id: "s".into(),
            turn_id: "t".into(),
            prompt: String::new(),
            plan_mode: None,
            model: None,
            evidence: None,
            retry_of_turn_id: None,
        })
        .await
        .unwrap();
        assert_eq!(power.held().len(), 1);
        bus.publish(CoreEvent::SessionTurnAborted {
            session_id: "s".into(),
            turn_id: "t".into(),
            reason: Some("usage limit".into()),
        })
        .await
        .unwrap();
        assert_eq!(label(&keep).as_deref(), Some("agente acabou de terminar"));
    }

    #[tokio::test]
    async fn run_lets_go_when_the_hand_off_grace_ends_not_at_the_next_reconcile() {
        struct Idle;
        #[async_trait]
        impl TurnSource for Idle {
            async fn active_turns(&self) -> Result<Vec<(String, String)>, String> {
                Ok(Vec::new())
            }
        }
        let power = Arc::new(FakePower::default());
        let keep = KeepAwake::new(
            power.clone(),
            None,
            KeepAwakeMode::DuringAgents,
            Duration::from_millis(300),
        );
        keep.turn_started("s", "t", Instant::now());
        tokio::spawn(keep.clone().run(Arc::new(Idle)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        keep.turn_ended("s", "t", Instant::now());
        assert_eq!(power.held().len(), 1, "held through the hand-off");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            power.held().is_empty(),
            "released well before RECONCILE_EVERY"
        );
    }

    /// The real IOKit layer: the assertion shows up for this process in
    /// `pmset -g assertions` and goes away on release.
    #[cfg(target_os = "macos")]
    #[test]
    fn system_power_creates_and_releases_a_real_assertion() {
        let name = format!("DCC keep-awake test {}", std::process::id());
        let listed = || {
            let output = std::process::Command::new("pmset")
                .args(["-g", "assertions"])
                .output()
                .expect("pmset");
            String::from_utf8_lossy(&output.stdout).contains(&name)
        };
        let id = SystemPower.acquire(&name).expect("assertion");
        assert!(listed(), "assertion missing from pmset");
        SystemPower.release(id);
        assert!(!listed(), "assertion still listed after release");
    }

    #[test]
    fn the_mode_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_mode(dir.path()), KeepAwakeMode::Off);
        let keep = KeepAwake::new(
            Arc::new(FakePower::default()),
            Some(dir.path().to_path_buf()),
            KeepAwakeMode::Off,
            GRACE,
        );
        keep.set_mode(KeepAwakeMode::DuringAgents).unwrap();
        assert_eq!(load_mode(dir.path()), KeepAwakeMode::DuringAgents);
        std::fs::write(dir.path().join(SETTINGS_FILE_NAME), "{not json").unwrap();
        assert_eq!(load_mode(dir.path()), KeepAwakeMode::Off);
    }
}
