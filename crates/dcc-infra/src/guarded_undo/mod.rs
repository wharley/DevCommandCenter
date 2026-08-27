//! Guarded Undo capture-v2 foundations.
//!
//! This module deliberately contains no workspace capture, database lifecycle,
//! prepare API, or workspace mutation. It only provides private-storage and
//! coordination primitives used by later reviewed phases.

pub mod coordinator;

// Phase 1A deliberately exports no operational filesystem adapter. Every
// current platform fails closed until a reviewed implementation exists.
pub mod unsupported;

pub use unsupported::{
    ArtifactStoreError, InstanceLock, PublishState, StagedArtifact, UnixArtifactStore,
    VerifiedArtifact,
};
