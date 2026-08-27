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
        let joins: Vec<_> = (0..16)
            .map(|id| {
                let c = Arc::clone(&c);
                thread::spawn(move || c.begin_turn_interval(root(5), owner(id)))
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
}
