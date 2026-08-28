//! Fail-closed in-process coordination keyed only by physical root ID.

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use dcc_core::domain::{
    guarded_undo::{GuardedUndoReasonCode, PhysicalRootId},
    session::{SessionId, TurnId},
};

const MAX_RETAINED_ROOT_GENERATIONS: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TurnReceiptId(u64);

/// Owner values are never rendered: session/turn text is user-originated.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TurnOwner {
    session_id: SessionId,
    turn_id: TurnId,
}

impl TurnOwner {
    pub fn new(session_id: SessionId, turn_id: TurnId) -> Self {
        Self {
            session_id,
            turn_id,
        }
    }
}

impl fmt::Debug for TurnOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TurnOwner([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnReceiptState {
    Clean { generation: u64 },
    Ineligible { reason_code: GuardedUndoReasonCode },
}

#[derive(Clone)]
pub struct TurnReceipt {
    id: TurnReceiptId,
    state: Arc<Mutex<TurnReceiptState>>,
}

impl TurnReceipt {
    pub fn id(&self) -> TurnReceiptId {
        self.id
    }

    pub fn state(&self) -> Result<TurnReceiptState, CoordinatorError> {
        Ok(lock(&self.state)?.clone())
    }
}

impl fmt::Debug for TurnReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("TurnReceipt");
        debug.field("id", &self.id);
        match self.state() {
            Ok(state) => debug.field("state", &state),
            Err(_) => debug.field("state", &"[unavailable]"),
        };
        debug.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoordinatorError {
    InvalidPhysicalRoot,
    Unavailable,
    ReceiptIdExhausted,
    GenerationExhausted,
    RootGenerationCapacityExhausted,
    DuplicateOwner,
    MutationInProgress,
    CaptureEdgeActive,
}

#[derive(Default)]
pub struct WorkspaceMutationCoordinator {
    state: Mutex<CoordinatorState>,
    next_receipt_id: AtomicU64,
}

#[derive(Default)]
struct CoordinatorState {
    roots: HashMap<PhysicalRootId, RootEntry>,
    // Never evict a generation. New roots are rejected at the explicit bound,
    // so an idle RootEntry may be removed without a later generation reset.
    generations: HashMap<PhysicalRootId, u64>,
    generation_order: VecDeque<PhysicalRootId>,
}

#[derive(Default)]
struct RootEntry {
    mutation_active: bool,
    active_capture_edges: u32,
    active_turns: HashMap<TurnOwner, ActiveTurn>,
}

struct ActiveTurn {
    state: Arc<Mutex<TurnReceiptState>>,
}

impl WorkspaceMutationCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_turn_interval(
        self: &Arc<Self>,
        root_id: PhysicalRootId,
        owner: TurnOwner,
    ) -> Result<TurnIntervalGuard, CoordinatorError> {
        validate_root(&root_id)?;
        let mut state = lock(&self.state)?;
        // Reserve the root before an ID allocation or any observable state
        // change. A known provider turn cannot begin during capture.
        if let Some(entry) = state.roots.get(&root_id) {
            if entry.active_capture_edges != 0 {
                return Err(CoordinatorError::CaptureEdgeActive);
            }
            if entry.active_turns.contains_key(&owner) {
                return Err(CoordinatorError::DuplicateOwner);
            }
        }
        let receipt_id = self.allocate_receipt_id()?;
        let generation = generation_for(&mut state, &root_id)?;
        let entry = state.roots.entry(root_id.clone()).or_default();
        let conflict = !entry.active_turns.is_empty() || entry.mutation_active;
        if conflict {
            mark_all_dirty(entry)?;
        }
        let receipt_state = Arc::new(Mutex::new(TurnReceiptState::Clean { generation }));
        if conflict {
            mark_dirty(&receipt_state)?;
        }
        entry.active_turns.insert(
            owner.clone(),
            ActiveTurn {
                state: Arc::clone(&receipt_state),
            },
        );
        drop(state);
        Ok(TurnIntervalGuard {
            coordinator: Arc::clone(self),
            root_id,
            owner,
            receipt: TurnReceipt {
                id: receipt_id,
                state: receipt_state,
            },
            active: true,
        })
    }

    /// Shared capture admission: a mutation cannot start while any edge lives.
    pub fn try_acquire_capture_edge(
        self: &Arc<Self>,
        root_id: &PhysicalRootId,
    ) -> Result<CaptureEdgeGuard, CoordinatorError> {
        validate_root(root_id)?;
        let mut state = lock(&self.state)?;
        generation_for(&mut state, root_id)?;
        let entry = state.roots.entry(root_id.clone()).or_default();
        if entry.mutation_active {
            return Err(CoordinatorError::MutationInProgress);
        }
        entry.active_capture_edges = entry
            .active_capture_edges
            .checked_add(1)
            .ok_or(CoordinatorError::GenerationExhausted)?;
        drop(state);
        Ok(CaptureEdgeGuard {
            coordinator: Arc::clone(self),
            root_id: root_id.clone(),
            active: true,
        })
    }

    pub fn try_acquire_mutation(
        self: &Arc<Self>,
        root_id: &PhysicalRootId,
    ) -> Result<MutationGuard, CoordinatorError> {
        validate_root(root_id)?;
        let mut state = lock(&self.state)?;
        let current = generation_for(&mut state, root_id)?;
        let entry = state.roots.entry(root_id.clone()).or_default();
        if entry.mutation_active {
            return Err(CoordinatorError::MutationInProgress);
        }
        if entry.active_capture_edges != 0 {
            return Err(CoordinatorError::CaptureEdgeActive);
        }
        let generation = current
            .checked_add(1)
            .ok_or(CoordinatorError::GenerationExhausted)?;
        mark_all_dirty(entry)?;
        entry.mutation_active = true;
        *state
            .generations
            .get_mut(root_id)
            .ok_or(CoordinatorError::Unavailable)? = generation;
        drop(state);
        Ok(MutationGuard {
            coordinator: Arc::clone(self),
            root_id: root_id.clone(),
            generation,
            active: true,
        })
    }

    /// Atomically acquires mutation ownership for all requested roots.
    ///
    /// Roots are validated, deduplicated, and sorted before taking the state
    /// lock. While holding that one lock, every edge/mutation conflict,
    /// generation capacity/overflow, and existing receipt lock is checked
    /// before any generation or receipt is changed. The returned guard drops
    /// all mutation ownership together, so a failed preflight cannot leave a
    /// partially acquired multi-root operation.
    pub fn try_acquire_mutations(
        self: &Arc<Self>,
        mut root_ids: Vec<PhysicalRootId>,
    ) -> Result<MultiMutationGuard, CoordinatorError> {
        if root_ids.is_empty() {
            return Err(CoordinatorError::InvalidPhysicalRoot);
        }
        for root_id in &root_ids {
            validate_root(root_id)?;
        }
        root_ids.sort_by(|left, right| left.0.cmp(&right.0));
        root_ids.dedup();

        let mut state = lock(&self.state)?;
        let mut plans = Vec::with_capacity(root_ids.len());
        for root_id in &root_ids {
            let current = state.generations.get(root_id).copied().unwrap_or(0);
            let next = current
                .checked_add(1)
                .ok_or(CoordinatorError::GenerationExhausted)?;
            if let Some(entry) = state.roots.get(root_id) {
                if entry.mutation_active {
                    return Err(CoordinatorError::MutationInProgress);
                }
                if entry.active_capture_edges != 0 {
                    return Err(CoordinatorError::CaptureEdgeActive);
                }
            }
            plans.push((root_id.clone(), next));
        }
        let new_generations = plans
            .iter()
            .filter(|(root_id, _)| !state.generations.contains_key(root_id))
            .count();
        if state.generations.len().saturating_add(new_generations) > MAX_RETAINED_ROOT_GENERATIONS {
            return Err(CoordinatorError::RootGenerationCapacityExhausted);
        }

        // Retain every receipt lock, in deterministic root/owner order, before
        // changing any receipt, generation, or mutation flag. In particular,
        // do not preflight with a temporary lock: another thread could poison
        // a later receipt between that check and the commit phase.
        let mut receipt_states = Vec::new();
        for root_id in &root_ids {
            if let Some(entry) = state.roots.get(root_id) {
                let mut owners = entry.active_turns.iter().collect::<Vec<_>>();
                owners.sort_by(|(left, _), (right, _)| {
                    left.session_id
                        .0
                        .as_bytes()
                        .cmp(right.session_id.0.as_bytes())
                        .then_with(|| left.turn_id.0.as_bytes().cmp(right.turn_id.0.as_bytes()))
                });
                receipt_states.extend(owners.into_iter().map(|(_, turn)| Arc::clone(&turn.state)));
            }
        }
        let mut receipt_guards = Vec::with_capacity(receipt_states.len());
        for receipt_state in &receipt_states {
            receipt_guards.push(
                receipt_state
                    .lock()
                    .map_err(|_| CoordinatorError::Unavailable)?,
            );
        }

        // All fallible checks are complete and all receipt guards remain held.
        // Mutations below are bounded map updates under the same mutex and
        // therefore become visible together.
        for receipt_guard in &mut receipt_guards {
            **receipt_guard = TurnReceiptState::Ineligible {
                reason_code: GuardedUndoReasonCode::ConcurrentWorkspaceMutation,
            };
        }
        let mut acquired = Vec::with_capacity(plans.len());
        for (root_id, generation) in plans {
            let entry = state.roots.entry(root_id.clone()).or_default();
            entry.mutation_active = true;
            if !state.generations.contains_key(&root_id) {
                state.generations.insert(root_id.clone(), 0);
                state.generation_order.push_back(root_id.clone());
            }
            if let Some(generation_slot) = state.generations.get_mut(&root_id) {
                *generation_slot = generation;
            } else {
                // The generation map was checked/updated above while holding
                // the state mutex. This branch is unreachable unless the
                // invariant is violated; never turn it into a fallible tail
                // after earlier roots have been mutated.
                unreachable!("generation plan missing during multi-root commit");
            }
            acquired.push((root_id, generation));
        }
        drop(state);
        Ok(MultiMutationGuard {
            coordinator: Arc::clone(self),
            roots: acquired,
            active: true,
        })
    }

    pub fn generation(&self, root_id: &PhysicalRootId) -> Result<u64, CoordinatorError> {
        validate_root(root_id)?;
        Ok(*lock(&self.state)?.generations.get(root_id).unwrap_or(&0))
    }

    fn allocate_receipt_id(&self) -> Result<TurnReceiptId, CoordinatorError> {
        self.next_receipt_id
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |next| {
                next.checked_add(1)
            })
            .map(|previous| TurnReceiptId(previous + 1))
            .map_err(|_| CoordinatorError::ReceiptIdExhausted)
    }

    fn end_turn_interval(
        &self,
        root_id: &PhysicalRootId,
        owner: &TurnOwner,
    ) -> Result<(), CoordinatorError> {
        let mut state = lock(&self.state)?;
        if let Some(entry) = state.roots.get_mut(root_id) {
            entry.active_turns.remove(owner);
        }
        remove_idle_root(&mut state, root_id);
        Ok(())
    }

    fn release_mutation(&self, root_id: &PhysicalRootId) {
        let Ok(mut state) = lock(&self.state) else {
            return;
        };
        if let Some(entry) = state.roots.get_mut(root_id) {
            entry.mutation_active = false;
        }
        remove_idle_root(&mut state, root_id);
    }
    fn release_mutations(&self, roots: &[(PhysicalRootId, u64)]) -> Result<(), CoordinatorError> {
        let mut state = lock(&self.state)?;
        for (root_id, _) in roots {
            if let Some(entry) = state.roots.get_mut(root_id) {
                entry.mutation_active = false;
            }
        }
        for (root_id, _) in roots {
            remove_idle_root(&mut state, root_id);
        }
        Ok(())
    }
    fn release_capture_edge(&self, root_id: &PhysicalRootId) {
        let Ok(mut state) = lock(&self.state) else {
            return;
        };
        if let Some(entry) = state.roots.get_mut(root_id) {
            if let Some(next) = entry.active_capture_edges.checked_sub(1) {
                entry.active_capture_edges = next;
            }
        }
        remove_idle_root(&mut state, root_id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, CoordinatorError> {
    mutex.lock().map_err(|_| CoordinatorError::Unavailable)
}
fn validate_root(root_id: &PhysicalRootId) -> Result<(), CoordinatorError> {
    root_id
        .validate()
        .map_err(|_| CoordinatorError::InvalidPhysicalRoot)
}
fn generation_for(
    state: &mut CoordinatorState,
    root_id: &PhysicalRootId,
) -> Result<u64, CoordinatorError> {
    if let Some(generation) = state.generations.get(root_id) {
        return Ok(*generation);
    }
    if state.generations.len() == MAX_RETAINED_ROOT_GENERATIONS {
        return Err(CoordinatorError::RootGenerationCapacityExhausted);
    }
    state.generations.insert(root_id.clone(), 0);
    state.generation_order.push_back(root_id.clone());
    Ok(0)
}
fn remove_idle_root(state: &mut CoordinatorState, root_id: &PhysicalRootId) {
    let idle = state.roots.get(root_id).is_some_and(|entry| {
        !entry.mutation_active && entry.active_capture_edges == 0 && entry.active_turns.is_empty()
    });
    if idle {
        state.roots.remove(root_id);
    }
}
fn mark_dirty(state: &Arc<Mutex<TurnReceiptState>>) -> Result<(), CoordinatorError> {
    *lock(state)? = TurnReceiptState::Ineligible {
        reason_code: GuardedUndoReasonCode::ConcurrentWorkspaceMutation,
    };
    Ok(())
}
fn mark_all_dirty(entry: &RootEntry) -> Result<(), CoordinatorError> {
    for turn in entry.active_turns.values() {
        mark_dirty(&turn.state)?;
    }
    Ok(())
}

pub struct TurnIntervalGuard {
    coordinator: Arc<WorkspaceMutationCoordinator>,
    root_id: PhysicalRootId,
    owner: TurnOwner,
    receipt: TurnReceipt,
    active: bool,
}
impl TurnIntervalGuard {
    pub fn receipt(&self) -> TurnReceipt {
        self.receipt.clone()
    }

    /// The explicit normal boundary. Cancellation and unwinding use `Drop` and
    /// are intentionally ineligible instead of silently preserving `Clean`.
    pub fn finish(mut self) -> Result<TurnReceipt, CoordinatorError> {
        match self
            .coordinator
            .end_turn_interval(&self.root_id, &self.owner)
        {
            Ok(()) => {
                self.active = false;
                Ok(self.receipt.clone())
            }
            Err(error) => {
                let _ = mark_dirty(&self.receipt.state);
                self.active = false;
                Err(error)
            }
        }
    }
}
impl Drop for TurnIntervalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = mark_dirty(&self.receipt.state);
            let _ = self
                .coordinator
                .end_turn_interval(&self.root_id, &self.owner);
            self.active = false;
        }
    }
}
pub struct CaptureEdgeGuard {
    coordinator: Arc<WorkspaceMutationCoordinator>,
    root_id: PhysicalRootId,
    active: bool,
}
impl Drop for CaptureEdgeGuard {
    fn drop(&mut self) {
        if self.active {
            self.coordinator.release_capture_edge(&self.root_id);
            self.active = false;
        }
    }
}
pub struct MutationGuard {
    coordinator: Arc<WorkspaceMutationCoordinator>,
    root_id: PhysicalRootId,
    generation: u64,
    active: bool,
}

/// RAII owner for one atomic multi-root mutation acquisition.
pub struct MultiMutationGuard {
    coordinator: Arc<WorkspaceMutationCoordinator>,
    roots: Vec<(PhysicalRootId, u64)>,
    active: bool,
}

impl MultiMutationGuard {
    pub fn generations(&self) -> Vec<u64> {
        self.roots
            .iter()
            .map(|(_, generation)| *generation)
            .collect()
    }

    pub fn finish(mut self) -> Result<Vec<u64>, CoordinatorError> {
        let generations = self.generations();
        match self.coordinator.release_mutations(&self.roots) {
            Ok(()) => {
                self.active = false;
                Ok(generations)
            }
            Err(error) => Err(error),
        }
    }
}

impl fmt::Debug for MultiMutationGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MultiMutationGuard")
            .field("count", &self.roots.len())
            .field("generations", &self.generations())
            .finish_non_exhaustive()
    }
}

impl Drop for MultiMutationGuard {
    fn drop(&mut self) {
        if self.active {
            self.coordinator.release_mutations(&self.roots).ok();
            self.active = false;
        }
    }
}

impl MutationGuard {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
impl fmt::Debug for MutationGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MutationGuard")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}
impl Drop for MutationGuard {
    fn drop(&mut self) {
        if self.active {
            self.coordinator.release_mutation(&self.root_id);
            self.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::Barrier,
        thread,
    };
    fn root(id: u8) -> PhysicalRootId {
        PhysicalRootId(vec![1, 1, id])
    }
    fn owner(id: u8) -> TurnOwner {
        TurnOwner::new(
            SessionId(format!("session-{id}")),
            TurnId(format!("turn-{id}")),
        )
    }
    #[test]
    fn overlap_dirties_all() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let first = c.begin_turn_interval(root(7), owner(1)).unwrap();
        let second = c.begin_turn_interval(root(7), owner(2)).unwrap();
        assert!(matches!(
            first.receipt().state(),
            Ok(TurnReceiptState::Ineligible { .. })
        ));
        assert!(matches!(
            second.receipt().state(),
            Ok(TurnReceiptState::Ineligible { .. })
        ));
    }
    #[test]
    fn duplicate_owner_is_non_destructive() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let first = c.begin_turn_interval(root(8), owner(1)).unwrap();
        assert!(matches!(
            c.begin_turn_interval(root(8), owner(1)),
            Err(CoordinatorError::DuplicateOwner)
        ));
        assert!(matches!(
            first.receipt().state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        ));
    }
    #[test]
    fn aliases_and_idle_removal_preserve_generation() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let one = c.try_acquire_mutation(&root(9)).unwrap();
        assert_eq!(one.generation(), 1);
        assert!(matches!(
            c.try_acquire_mutation(&root(9)),
            Err(CoordinatorError::MutationInProgress)
        ));
        drop(one);
        assert!(lock(&c.state).unwrap().roots.is_empty());
        assert_eq!(c.try_acquire_mutation(&root(9)).unwrap().generation(), 2);
    }
    #[test]
    fn edge_blocks_mutation_and_raii_releases() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let edge = c.try_acquire_capture_edge(&root(2)).unwrap();
        assert!(matches!(
            c.try_acquire_mutation(&root(2)),
            Err(CoordinatorError::CaptureEdgeActive)
        ));
        drop(edge);
        assert!(c.try_acquire_mutation(&root(2)).is_ok());
    }

    #[test]
    fn capture_edge_rejects_turn_without_dirtying_an_existing_owner() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let existing = c.begin_turn_interval(root(12), owner(1)).unwrap();
        let receipt = existing.receipt();
        let edge = c.try_acquire_capture_edge(&root(12)).unwrap();
        assert!(matches!(
            c.begin_turn_interval(root(12), owner(2)),
            Err(CoordinatorError::CaptureEdgeActive)
        ));
        assert!(matches!(
            receipt.state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        ));
        drop(edge);
        let fresh_edge = c.try_acquire_capture_edge(&root(13)).unwrap();
        assert!(matches!(
            c.begin_turn_interval(root(13), owner(3)),
            Err(CoordinatorError::CaptureEdgeActive)
        ));
        drop(fresh_edge);
        let clean = c
            .begin_turn_interval(root(13), owner(3))
            .unwrap()
            .finish()
            .unwrap();
        assert!(matches!(
            clean.state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        ));
        let _ = existing.finish();
    }

    #[test]
    fn normal_finish_preserves_clean_receipt_while_drop_dirties() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let interval = c.begin_turn_interval(root(6), owner(1)).unwrap();
        let receipt = interval.receipt();
        let finished = interval.finish().unwrap();
        assert!(matches!(
            finished.state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        ));
        let cancelled = c.begin_turn_interval(root(6), owner(2)).unwrap();
        let cancelled_receipt = cancelled.receipt();
        drop(cancelled);
        assert!(matches!(
            cancelled_receipt.state(),
            Ok(TurnReceiptState::Ineligible {
                reason_code: GuardedUndoReasonCode::ConcurrentWorkspaceMutation
            })
        ));
        assert_eq!(receipt.state(), finished.state());
    }
    #[test]
    fn overflow_is_typed() {
        let c = WorkspaceMutationCoordinator::new();
        c.next_receipt_id.store(u64::MAX, Ordering::Release);
        assert_eq!(
            c.allocate_receipt_id(),
            Err(CoordinatorError::ReceiptIdExhausted)
        );
        lock(&c.state)
            .unwrap()
            .generations
            .insert(root(4), u64::MAX);
        assert!(matches!(
            Arc::new(c).try_acquire_mutation(&root(4)),
            Err(CoordinatorError::GenerationExhausted)
        ));
    }
    #[test]
    fn poison_is_unavailable_and_drop_does_not_panic() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let interval = c.begin_turn_interval(root(3), owner(1)).unwrap();
        let receipt = interval.receipt();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = c.state.lock().unwrap();
            panic!("poison");
        }));
        assert_eq!(c.generation(&root(3)), Err(CoordinatorError::Unavailable));
        assert!(catch_unwind(AssertUnwindSafe(|| drop(interval))).is_ok());
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = receipt.state.lock().unwrap();
            panic!("poison");
        }));
        assert_eq!(receipt.state(), Err(CoordinatorError::Unavailable));
        assert!(format!("{receipt:?}").contains("unavailable"));
    }
    #[test]
    fn concurrent_intervals_do_not_panic() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let ready = Arc::new(Barrier::new(16));
        let joins: Vec<_> = (0..16)
            .map(|id| {
                let c = Arc::clone(&c);
                let ready = Arc::clone(&ready);
                thread::spawn(move || {
                    let interval = c.begin_turn_interval(root(5), owner(id));
                    ready.wait();
                    interval
                })
            })
            .collect();
        for join in joins {
            let guard = join.join().unwrap().unwrap();
            assert!(matches!(
                guard.receipt().state(),
                Ok(TurnReceiptState::Ineligible { .. })
            ));
        }
    }

    #[test]
    fn multi_mutation_rejects_empty_and_deduplicates_roots() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        assert!(matches!(
            c.try_acquire_mutations(Vec::new()),
            Err(CoordinatorError::InvalidPhysicalRoot)
        ));
        let guard = c
            .try_acquire_mutations(vec![root(20), root(20), root(19)])
            .unwrap();
        assert_eq!(guard.generations(), vec![1, 1]);
        assert!(format!("{guard:?}").contains("count: 2"));
        let state = lock(&c.state).unwrap();
        assert_eq!(
            state.generation_order.back(),
            Some(&root(20)),
            "roots are committed in deterministic byte order"
        );
        drop(state);
        assert_eq!(guard.finish().unwrap(), vec![1, 1]);
        assert!(lock(&c.state).unwrap().roots.is_empty());
    }

    #[test]
    fn multi_mutation_second_edge_failure_is_all_or_none() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let edge = c.try_acquire_capture_edge(&root(22)).unwrap();
        assert!(matches!(
            c.try_acquire_mutations(vec![root(21), root(22)]),
            Err(CoordinatorError::CaptureEdgeActive)
        ));
        assert_eq!(c.generation(&root(21)), Ok(0));
        assert_eq!(c.generation(&root(22)), Ok(0));
        assert!(lock(&c.state).unwrap().roots.get(&root(21)).is_none());
        drop(edge);
    }

    #[test]
    fn multi_mutation_existing_turns_are_dirtied_only_after_all_checks() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let first = c.begin_turn_interval(root(23), owner(1)).unwrap();
        let first_receipt = first.receipt();
        let edge = c.try_acquire_capture_edge(&root(24)).unwrap();
        assert!(matches!(
            c.try_acquire_mutations(vec![root(23), root(24)]),
            Err(CoordinatorError::CaptureEdgeActive)
        ));
        assert!(matches!(
            first_receipt.state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        ));
        drop(edge);
        drop(first);
    }

    #[test]
    fn multi_mutation_poisoned_later_receipt_is_non_destructive() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let first_root = root(30);
        let second_root = root(31);
        let first = c.begin_turn_interval(first_root.clone(), owner(1)).unwrap();
        let second_owner = owner(2);
        let second = c
            .begin_turn_interval(second_root.clone(), second_owner.clone())
            .unwrap();
        let first_receipt = first.receipt();
        let second_state = {
            let state = lock(&c.state).unwrap();
            Arc::clone(
                &state
                    .roots
                    .get(&second_root)
                    .unwrap()
                    .active_turns
                    .get(&second_owner)
                    .unwrap()
                    .state,
            )
        };
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = second_state.lock().unwrap();
            panic!("poison receipt");
        }));

        assert!(matches!(
            c.try_acquire_mutations(vec![first_root.clone(), second_root.clone()]),
            Err(CoordinatorError::Unavailable)
        ));
        assert_eq!(
            first_receipt.state(),
            Ok(TurnReceiptState::Clean { generation: 0 })
        );
        assert_eq!(c.generation(&first_root), Ok(0));
        assert_eq!(c.generation(&second_root), Ok(0));
        let state = lock(&c.state).unwrap();
        assert!(!state.roots.get(&first_root).unwrap().mutation_active);
        drop(state);
        drop(second);
        drop(first);
    }

    #[test]
    fn multi_mutation_drop_and_unwind_release_all_roots() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        let result = catch_unwind(AssertUnwindSafe({
            let c = Arc::clone(&c);
            move || {
                let _guard = c.try_acquire_mutations(vec![root(25), root(26)]).unwrap();
                panic!("test unwind");
            }
        }));
        assert!(result.is_err());
        assert!(lock(&c.state).unwrap().roots.is_empty());
        let guard = c.try_acquire_mutations(vec![root(26), root(25)]).unwrap();
        assert_eq!(guard.generations(), vec![2, 2]);
    }

    #[test]
    fn multi_mutation_generation_overflow_is_non_destructive() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        lock(&c.state)
            .unwrap()
            .generations
            .insert(root(27), u64::MAX);
        assert!(matches!(
            c.try_acquire_mutations(vec![root(28), root(27)]),
            Err(CoordinatorError::GenerationExhausted)
        ));
        assert_eq!(c.generation(&root(28)), Ok(0));
        assert!(lock(&c.state).unwrap().roots.get(&root(28)).is_none());
        assert!(!lock(&c.state).unwrap().roots.contains_key(&root(27)));
    }

    #[test]
    fn multi_mutation_generation_capacity_is_non_destructive() {
        let c = Arc::new(WorkspaceMutationCoordinator::new());
        {
            let mut state = lock(&c.state).unwrap();
            for index in 0..MAX_RETAINED_ROOT_GENERATIONS {
                let mut bytes = vec![1, 1];
                bytes.extend_from_slice(&(index as u64).to_be_bytes());
                let root_id = PhysicalRootId(bytes);
                state.generations.insert(root_id.clone(), 0);
                state.generation_order.push_back(root_id);
            }
        }
        assert!(matches!(
            c.try_acquire_mutations(vec![root(29)]),
            Err(CoordinatorError::RootGenerationCapacityExhausted)
        ));
        assert_eq!(c.generation(&root(29)), Ok(0));
        assert!(lock(&c.state).unwrap().roots.get(&root(29)).is_none());
    }
}
