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
pub(crate) mod macos_restore_adapter;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod macos_root;
#[cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]
pub mod macos_store;
#[cfg(feature = "guarded-undo-capture-v2")]
pub mod restore_service;

// The legacy public placeholder remains fail-closed for unsupported platforms;
// operational restore authority is private to the reviewed macOS adapter.
pub mod unsupported;

pub use unsupported::{
    ArtifactStoreError, InstanceLock, PublishState, StagedArtifact, UnixArtifactStore,
    VerifiedArtifact,
};
