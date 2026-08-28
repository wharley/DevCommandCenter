//! Process-local, bounded coalescing for terminal turn work.
//!
//! The first valid intent to claim a `(session, turn)` key wins. In
//! particular, a quiesce path may claim `Aborted` before it invokes provider
//! cancellation; a later completion/provider-failure path then follows the
//! committed outcome and never runs terminal evidence or append work.
//!
//! This arbiter intentionally remembers only a bounded recent window. SQLite
//! terminal uniqueness and lookup remain authoritative after tombstone
//! eviction or process restart; this component does not promise forever
//! exactly-once behavior by itself.

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use dcc_core::domain::session::{SessionId, TurnId};
use tokio::sync::Notify;

const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
const WAIT_SLICE: Duration = Duration::from_millis(25);
const MAX_ACTIVE_CLAIMS: usize = 1_024;
const MAX_COMMITTED_TOMBSTONES: usize = 4_096;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TerminalKey {
    pub session_id: SessionId,
    pub turn_id: TurnId,
}

impl fmt::Debug for TerminalKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TerminalKey([redacted])")
    }
}

impl TerminalKey {
    pub fn new(session_id: SessionId, turn_id: TurnId) -> Self {
        Self {
            session_id,
            turn_id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalIntent {
    Completed,
    Aborted,
    ProviderFailed,
}

pub type TerminalOutcome = TerminalIntent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalArbiterError {
    TimedOut,
    Poisoned,
    ClaimCapacityExhausted,
    StaleClaim,
}

impl fmt::Display for TerminalArbiterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TimedOut => "terminal claim timed out",
            Self::Poisoned => "terminal arbiter unavailable",
            Self::ClaimCapacityExhausted => "terminal claim capacity exhausted",
            Self::StaleClaim => "terminal claim is no longer current",
        })
    }
}

impl std::error::Error for TerminalArbiterError {}

#[derive(PartialEq, Eq)]
pub enum PersistThenCommitError<E> {
    Persistence(E),
    Arbiter(TerminalArbiterError),
}

impl<E> fmt::Debug for PersistThenCommitError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(_) => formatter.write_str("Persistence([redacted])"),
            Self::Arbiter(error) => formatter.debug_tuple("Arbiter").field(error).finish(),
        }
    }
}

impl<E: fmt::Display> fmt::Display for PersistThenCommitError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Persistence(_) => formatter.write_str("terminal persistence failed"),
            Self::Arbiter(error) => error.fmt(formatter),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for PersistThenCommitError<E> {}

pub enum TerminalClaimResult {
    Leader(TerminalClaim),
    AlreadyCommitted(TerminalOutcome),
}

impl fmt::Debug for TerminalClaimResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Leader(claim) => formatter
                .debug_tuple("Leader")
                .field(&claim.intent())
                .finish(),
            Self::AlreadyCommitted(outcome) => formatter
                .debug_tuple("AlreadyCommitted")
                .field(outcome)
                .finish(),
        }
    }
}

struct Entry {
    owner: u64,
    intent: TerminalIntent,
    notify: Notify,
}

#[derive(Default)]
struct Registry {
    // Active entries contain only synchronization state and are removed on
    // commit or release. Committed outcomes are lightweight tombstones needed
    // for recent idempotent replay; they contain no reason or other sensitive
    // data. FIFO eviction is safe because an evicted replay may lead again,
    // but the persistence closure's durable idempotency remains authoritative.
    active: HashMap<TerminalKey, Arc<Entry>>,
    committed: HashMap<TerminalKey, CommittedRecord>,
    committed_order: VecDeque<(TerminalKey, u64)>,
}

#[derive(Clone, Copy)]
struct CommittedRecord {
    outcome: TerminalOutcome,
    generation: u64,
}

struct Inner {
    registry: Mutex<Registry>,
    next_owner: AtomicU64,
    wait_timeout: Duration,
}

#[derive(Clone)]
pub struct TerminalArbiter {
    inner: Arc<Inner>,
}

impl Default for TerminalArbiter {
    fn default() -> Self {
        Self::new(DEFAULT_WAIT_TIMEOUT)
    }
}

impl fmt::Debug for TerminalArbiter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalArbiter")
            .finish_non_exhaustive()
    }
}

impl TerminalArbiter {
    pub fn new(wait_timeout: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry: Mutex::new(Registry::default()),
                next_owner: AtomicU64::new(1),
                wait_timeout,
            }),
        }
    }

    /// Claims ownership without holding the registry lock across caller work.
    /// Followers wait only for bounded slices and retry the authoritative map,
    /// so cancellation never registers waiter state that needs cleanup.
    pub async fn claim(
        &self,
        key: TerminalKey,
        intent: TerminalIntent,
    ) -> Result<TerminalClaimResult, TerminalArbiterError> {
        let started = Instant::now();
        loop {
            let entry = {
                let mut registry = self
                    .inner
                    .registry
                    .lock()
                    .map_err(|_| TerminalArbiterError::Poisoned)?;
                if let Some(record) = registry.committed.get(&key).copied() {
                    return Ok(TerminalClaimResult::AlreadyCommitted(record.outcome));
                }
                if let Some(entry) = registry.active.get(&key) {
                    Arc::clone(entry)
                } else {
                    if registry.active.len() >= MAX_ACTIVE_CLAIMS {
                        return Err(TerminalArbiterError::ClaimCapacityExhausted);
                    }
                    let owner = self
                        .inner
                        .next_owner
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                            value.checked_add(1)
                        })
                        .map_err(|_| TerminalArbiterError::ClaimCapacityExhausted)?;
                    let entry = Arc::new(Entry {
                        owner,
                        intent,
                        notify: Notify::new(),
                    });
                    registry.active.insert(key.clone(), Arc::clone(&entry));
                    return Ok(TerminalClaimResult::Leader(TerminalClaim {
                        inner: Arc::clone(&self.inner),
                        key,
                        entry,
                        active: true,
                    }));
                }
            };

            // Register before the authoritative recheck to close the
            // notify-before-await race. A missed wake can at worst consume one
            // bounded slice, never block indefinitely.
            let notified = entry.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let registry = self
                    .inner
                    .registry
                    .lock()
                    .map_err(|_| TerminalArbiterError::Poisoned)?;
                if let Some(record) = registry.committed.get(&key).copied() {
                    return Ok(TerminalClaimResult::AlreadyCommitted(record.outcome));
                }
                if !registry
                    .active
                    .get(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &entry))
                {
                    continue;
                }
            }

            let elapsed = started.elapsed();
            let remaining = self
                .inner
                .wait_timeout
                .checked_sub(elapsed)
                .ok_or(TerminalArbiterError::TimedOut)?;
            let slice = remaining.min(WAIT_SLICE);
            if tokio::time::timeout(slice, &mut notified).await.is_err()
                && started.elapsed() >= self.inner.wait_timeout
            {
                return Err(TerminalArbiterError::TimedOut);
            }
        }
    }

    #[cfg(test)]
    fn counts(&self) -> (usize, usize) {
        let registry = self.inner.registry.lock().unwrap();
        (registry.active.len(), registry.committed.len())
    }

    #[cfg(test)]
    fn poison(&self) {
        let _guard = self.inner.registry.lock().unwrap();
        panic!("intentional terminal arbiter poison");
    }
}

pub struct TerminalClaim {
    inner: Arc<Inner>,
    key: TerminalKey,
    entry: Arc<Entry>,
    active: bool,
}

impl fmt::Debug for TerminalClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TerminalClaim")
            .field("intent", &self.entry.intent)
            .finish_non_exhaustive()
    }
}

impl TerminalClaim {
    pub fn intent(&self) -> TerminalIntent {
        self.entry.intent
    }

    /// Runs the caller's durable precheck/append first and records the
    /// in-memory terminal tombstone only after that operation returns `Ok`.
    ///
    /// The persistence future executes with no arbiter mutex held. If it
    /// returns `Err`, panics, or is cancelled, this claim's RAII drop releases
    /// leadership and a later caller may retry the durable append.
    ///
    /// Integration protocol: a leader MUST query durable terminal state before
    /// running evidence work. If a terminal row/event already exists, the
    /// closure MUST skip evidence and return its canonical outcome. Otherwise
    /// it may collect evidence, durably append with a uniqueness key, and
    /// return the outcome inserted (or concurrently found). The returned
    /// canonical outcome, not necessarily the leader's intent, is tombstoned.
    pub async fn persist_then_commit<F, Fut, E>(
        mut self,
        persist: F,
    ) -> Result<TerminalOutcome, PersistThenCommitError<E>>
    where
        F: FnOnce(TerminalIntent) -> Fut,
        Fut: Future<Output = Result<TerminalOutcome, E>>,
    {
        let canonical_outcome = persist(self.entry.intent)
            .await
            .map_err(PersistThenCommitError::Persistence)?;
        self.commit_after_persistence(canonical_outcome)
            .map_err(PersistThenCommitError::Arbiter)
    }

    fn commit_after_persistence(
        &mut self,
        canonical_outcome: TerminalOutcome,
    ) -> Result<TerminalOutcome, TerminalArbiterError> {
        let mut registry = self
            .inner
            .registry
            .lock()
            .map_err(|_| TerminalArbiterError::Poisoned)?;
        let is_current = registry.active.get(&self.key).is_some_and(|current| {
            Arc::ptr_eq(current, &self.entry) && current.owner == self.entry.owner
        });
        if !is_current {
            return Err(TerminalArbiterError::StaleClaim);
        }
        registry.active.remove(&self.key);
        registry.committed.insert(
            self.key.clone(),
            CommittedRecord {
                outcome: canonical_outcome,
                generation: self.entry.owner,
            },
        );
        registry
            .committed_order
            .push_back((self.key.clone(), self.entry.owner));
        while registry.committed.len() > MAX_COMMITTED_TOMBSTONES {
            let Some((candidate, generation)) = registry.committed_order.pop_front() else {
                return Err(TerminalArbiterError::Poisoned);
            };
            if registry
                .committed
                .get(&candidate)
                .is_some_and(|record| record.generation == generation)
            {
                registry.committed.remove(&candidate);
            }
        }
        self.active = false;
        drop(registry);
        self.entry.notify.notify_waiters();
        Ok(canonical_outcome)
    }

    fn release(&mut self) {
        let Ok(mut registry) = self.inner.registry.lock() else {
            // A poisoned registry is fail-closed: never clear ownership based
            // on state that can no longer be trusted.
            return;
        };
        let is_current = registry.active.get(&self.key).is_some_and(|current| {
            Arc::ptr_eq(current, &self.entry) && current.owner == self.entry.owner
        });
        if is_current {
            registry.active.remove(&self.key);
        }
        drop(registry);
        self.entry.notify.notify_waiters();
    }

    #[cfg(test)]
    fn release_but_remain_stale_for_aba_test(&mut self) {
        self.release();
    }
}

impl Drop for TerminalClaim {
    fn drop(&mut self) {
        if self.active {
            self.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{oneshot, Barrier};

    fn key(session: &str, turn: &str) -> TerminalKey {
        TerminalKey::new(SessionId(session.to_owned()), TurnId(turn.to_owned()))
    }

    fn leader(result: TerminalClaimResult) -> TerminalClaim {
        match result {
            TerminalClaimResult::Leader(claim) => claim,
            TerminalClaimResult::AlreadyCommitted(outcome) => {
                panic!("expected leader, got {outcome:?}")
            }
        }
    }

    async fn persist_ok(claim: TerminalClaim) -> TerminalOutcome {
        claim
            .persist_then_commit(|intent| async move { Ok::<_, ()>(intent) })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn one_hundred_completion_abort_racers_have_one_leader_and_commit() {
        let arbiter = Arc::new(TerminalArbiter::new(Duration::from_secs(1)));
        let barrier = Arc::new(Barrier::new(100));
        let leaders = Arc::new(AtomicUsize::new(0));
        let durable_appends = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for index in 0..100 {
            let arbiter = Arc::clone(&arbiter);
            let barrier = Arc::clone(&barrier);
            let leaders = Arc::clone(&leaders);
            let durable_appends = Arc::clone(&durable_appends);
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                let intent = if index % 2 == 0 {
                    TerminalIntent::Completed
                } else {
                    TerminalIntent::Aborted
                };
                match arbiter.claim(key("s", "t"), intent).await.unwrap() {
                    TerminalClaimResult::Leader(claim) => {
                        leaders.fetch_add(1, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        claim
                            .persist_then_commit(move |intent| async move {
                                durable_appends.fetch_add(1, Ordering::SeqCst);
                                Ok::<_, ()>(intent)
                            })
                            .await
                            .unwrap()
                    }
                    TerminalClaimResult::AlreadyCommitted(outcome) => outcome,
                }
            }));
        }
        let mut outcomes = Vec::new();
        for task in tasks {
            outcomes.push(task.await.unwrap());
        }
        assert_eq!(leaders.load(Ordering::SeqCst), 1);
        assert_eq!(durable_appends.load(Ordering::SeqCst), 1);
        assert!(outcomes.iter().all(|value| *value == outcomes[0]));
        assert_eq!(arbiter.counts(), (0, 1));
    }

    #[tokio::test]
    async fn cancelling_leader_future_releases_claim_for_retry() {
        let arbiter = Arc::new(TerminalArbiter::new(Duration::from_secs(1)));
        let (claimed_tx, claimed_rx) = oneshot::channel();
        let task = {
            let arbiter = Arc::clone(&arbiter);
            tokio::spawn(async move {
                let claim = leader(
                    arbiter
                        .claim(key("s", "cancelled"), TerminalIntent::Completed)
                        .await
                        .unwrap(),
                );
                claimed_tx.send(()).unwrap();
                std::future::pending::<()>().await;
                drop(claim);
            })
        };
        claimed_rx.await.unwrap();
        task.abort();
        let _ = task.await;
        let retry = leader(
            arbiter
                .claim(key("s", "cancelled"), TerminalIntent::ProviderFailed)
                .await
                .unwrap(),
        );
        assert_eq!(retry.intent(), TerminalIntent::ProviderFailed);
        persist_ok(retry).await;
    }

    #[tokio::test]
    async fn committed_replay_is_idempotent_and_active_entry_is_cleaned() {
        let arbiter = TerminalArbiter::default();
        persist_ok(leader(
            arbiter
                .claim(key("s", "replay"), TerminalIntent::Completed)
                .await
                .unwrap(),
        ))
        .await;
        assert!(matches!(
            arbiter
                .claim(key("s", "replay"), TerminalIntent::Aborted)
                .await
                .unwrap(),
            TerminalClaimResult::AlreadyCommitted(TerminalIntent::Completed)
        ));
        assert_eq!(arbiter.counts(), (0, 1));
    }

    #[tokio::test]
    async fn distinct_turns_run_in_parallel_and_shared_owners_need_no_binding() {
        let shared = Arc::new(TerminalArbiter::default());
        let owner_one = Arc::clone(&shared);
        let owner_two = Arc::clone(&shared);
        let first = leader(
            owner_one
                .claim(key("s", "one"), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        let second = leader(
            owner_two
                .claim(key("s", "two"), TerminalIntent::Aborted)
                .await
                .unwrap(),
        );
        assert_eq!(shared.counts(), (2, 0));
        persist_ok(first).await;
        persist_ok(second).await;
    }

    #[tokio::test]
    async fn stale_drop_cannot_clear_successor_claim_after_aba() {
        let arbiter = TerminalArbiter::default();
        let mut stale = leader(
            arbiter
                .claim(key("s", "aba"), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        stale.release_but_remain_stale_for_aba_test();
        let successor = leader(
            arbiter
                .claim(key("s", "aba"), TerminalIntent::Aborted)
                .await
                .unwrap(),
        );
        drop(stale);
        assert_eq!(arbiter.counts(), (1, 0));
        assert_eq!(persist_ok(successor).await, TerminalIntent::Aborted);
    }

    #[tokio::test]
    async fn follower_times_out_without_sticking_and_poison_fails_closed() {
        let arbiter = Arc::new(TerminalArbiter::new(Duration::from_millis(20)));
        let leader_claim = leader(
            arbiter
                .claim(key("s", "timeout"), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        assert_eq!(
            arbiter
                .claim(key("s", "timeout"), TerminalIntent::Aborted)
                .await
                .unwrap_err(),
            TerminalArbiterError::TimedOut
        );
        drop(leader_claim);
        persist_ok(leader(
            arbiter
                .claim(key("s", "timeout"), TerminalIntent::Aborted)
                .await
                .unwrap(),
        ))
        .await;

        let poisoned = Arc::new(TerminalArbiter::default());
        let poisoner = {
            let poisoned = Arc::clone(&poisoned);
            std::thread::spawn(move || poisoned.poison())
        };
        assert!(poisoner.join().is_err());
        assert_eq!(
            poisoned
                .claim(key("s", "poisoned"), TerminalIntent::Completed)
                .await
                .unwrap_err(),
            TerminalArbiterError::Poisoned
        );
    }

    #[tokio::test]
    async fn persistence_failure_and_cancellation_release_for_retry() {
        let arbiter = Arc::new(TerminalArbiter::default());
        let failed = leader(
            arbiter
                .claim(key("s", "persist-fail"), TerminalIntent::Completed)
                .await
                .unwrap(),
        )
        .persist_then_commit(|_| async { Err::<TerminalOutcome, _>("append failed") })
        .await;
        assert_eq!(
            failed,
            Err(PersistThenCommitError::Persistence("append failed"))
        );
        let retry = leader(
            arbiter
                .claim(key("s", "persist-fail"), TerminalIntent::Aborted)
                .await
                .unwrap(),
        );
        assert_eq!(persist_ok(retry).await, TerminalIntent::Aborted);

        let claim = leader(
            arbiter
                .claim(key("s", "persist-cancel"), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        let (started_tx, started_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            claim
                .persist_then_commit(|_| async move {
                    started_tx.send(()).unwrap();
                    std::future::pending::<Result<TerminalOutcome, ()>>().await
                })
                .await
        });
        started_rx.await.unwrap();
        task.abort();
        let _ = task.await;
        let retry = leader(
            arbiter
                .claim(key("s", "persist-cancel"), TerminalIntent::ProviderFailed)
                .await
                .unwrap(),
        );
        assert_eq!(persist_ok(retry).await, TerminalIntent::ProviderFailed);
    }

    #[tokio::test]
    async fn active_capacity_is_bounded_and_released_capacity_is_reusable() {
        let arbiter = TerminalArbiter::default();
        let mut claims = Vec::with_capacity(MAX_ACTIVE_CLAIMS);
        for index in 0..MAX_ACTIVE_CLAIMS {
            claims.push(leader(
                arbiter
                    .claim(
                        key("capacity", &index.to_string()),
                        TerminalIntent::Completed,
                    )
                    .await
                    .unwrap(),
            ));
        }
        assert_eq!(arbiter.counts(), (MAX_ACTIVE_CLAIMS, 0));
        assert_eq!(
            arbiter
                .claim(key("capacity", "overflow"), TerminalIntent::Completed)
                .await
                .unwrap_err(),
            TerminalArbiterError::ClaimCapacityExhausted
        );
        drop(claims.pop());
        let replacement = leader(
            arbiter
                .claim(key("capacity", "replacement"), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        drop(replacement);
        drop(claims);
        assert_eq!(arbiter.counts(), (0, 0));
    }

    #[tokio::test]
    async fn evicted_key_rechecks_durable_authority_and_skips_evidence() {
        let arbiter = TerminalArbiter::default();
        let durable = Arc::new(Mutex::new(HashMap::<TerminalKey, TerminalOutcome>::new()));
        let evidence_runs = Arc::new(AtomicUsize::new(0));
        let evicted_key = key("eviction", "0");
        let initial = leader(
            arbiter
                .claim(evicted_key.clone(), TerminalIntent::Completed)
                .await
                .unwrap(),
        );
        let initial_outcome = initial
            .persist_then_commit({
                let durable = Arc::clone(&durable);
                let evidence_runs = Arc::clone(&evidence_runs);
                let evicted_key = evicted_key.clone();
                move |intent| async move {
                    let mut durable = durable.lock().unwrap();
                    if let Some(outcome) = durable.get(&evicted_key).copied() {
                        return Ok::<_, ()>(outcome);
                    }
                    evidence_runs.fetch_add(1, Ordering::SeqCst);
                    durable.insert(evicted_key, intent);
                    Ok(intent)
                }
            })
            .await
            .unwrap();
        assert_eq!(initial_outcome, TerminalIntent::Completed);

        for index in 1..=MAX_COMMITTED_TOMBSTONES {
            let claim = leader(
                arbiter
                    .claim(
                        key("eviction", &index.to_string()),
                        TerminalIntent::Completed,
                    )
                    .await
                    .unwrap(),
            );
            persist_ok(claim).await;
        }
        assert_eq!(arbiter.counts(), (0, MAX_COMMITTED_TOMBSTONES));
        let evicted = leader(
            arbiter
                .claim(evicted_key.clone(), TerminalIntent::Aborted)
                .await
                .unwrap(),
        );
        let canonical = evicted
            .persist_then_commit({
                let durable = Arc::clone(&durable);
                let evidence_runs = Arc::clone(&evidence_runs);
                let evicted_key = evicted_key.clone();
                move |intent| async move {
                    let mut durable = durable.lock().unwrap();
                    if let Some(outcome) = durable.get(&evicted_key).copied() {
                        return Ok::<_, ()>(outcome);
                    }
                    evidence_runs.fetch_add(1, Ordering::SeqCst);
                    durable.insert(evicted_key, intent);
                    Ok(intent)
                }
            })
            .await
            .unwrap();
        assert_eq!(canonical, TerminalIntent::Completed);
        assert_eq!(evidence_runs.load(Ordering::SeqCst), 1);
        assert!(matches!(
            arbiter
                .claim(evicted_key, TerminalIntent::ProviderFailed)
                .await
                .unwrap(),
            TerminalClaimResult::AlreadyCommitted(TerminalIntent::Completed)
        ));
        assert!(matches!(
            arbiter
                .claim(
                    key("eviction", &MAX_COMMITTED_TOMBSTONES.to_string()),
                    TerminalIntent::Aborted,
                )
                .await
                .unwrap(),
            TerminalClaimResult::AlreadyCommitted(TerminalIntent::Completed)
        ));
    }

    #[test]
    fn key_debug_is_redacted() {
        let debug = format!("{:?}", key("secret-session", "secret-turn"));
        assert_eq!(debug, "TerminalKey([redacted])");
        assert!(!debug.contains("secret"));

        let error = PersistThenCommitError::Persistence("secret database reason");
        let debug = format!("{error:?}");
        assert_eq!(debug, "Persistence([redacted])");
        assert!(!debug.contains("database"));
    }
}
