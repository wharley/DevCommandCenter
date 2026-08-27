//! Guarded Undo capture-v2 foundations.
//!
//! This module deliberately contains no workspace capture, database lifecycle,
//! prepare API, or workspace mutation. It only provides private-storage and
//! coordination primitives used by later reviewed phases.

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod capture_v2_service;
pub mod coordinator;
#[cfg(feature = "guarded-undo-capture-v2")]
pub mod git_inspector;

#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod macos_git_bridge;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod macos_root;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod macos_store;

// Phase 1A deliberately exports no operational filesystem adapter. Every
// current platform fails closed until a reviewed implementation exists.
pub mod unsupported;

pub use unsupported::{
    ArtifactStoreError, InstanceLock, PublishState, StagedArtifact, UnixArtifactStore,
    VerifiedArtifact,
};
