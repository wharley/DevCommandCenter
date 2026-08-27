//! Explicit fail-closed placeholder for every target without a reviewed adapter.

use std::{fmt, io::Read, path::Path};

use dcc_core::domain::guarded_undo::{ArtifactKey, GuardedUndoReasonCode, Sha256Digest};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("guarded undo capture-v2 filesystem adapter is unsupported on this platform")]
pub struct ArtifactStoreError;

impl ArtifactStoreError {
    pub fn reason_code(&self) -> GuardedUndoReasonCode {
        GuardedUndoReasonCode::AdapterUnsupported
    }
}

#[derive(Debug)]
pub struct InstanceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedArtifact {
    pub key: ArtifactKey,
    pub size: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedArtifact {
    pub key: ArtifactKey,
    pub size: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublishState {
    Published(VerifiedArtifact),
    PublishedCleanupPending(StagedArtifact),
}

pub struct UnixArtifactStore;

impl fmt::Debug for UnixArtifactStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnixArtifactStore").finish_non_exhaustive()
    }
}

impl UnixArtifactStore {
    pub fn open(_app_data_dir: &Path) -> Result<Self, ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn stage_reader<R: Read>(
        &self,
        _reader: &mut R,
        _maximum_bytes: u64,
    ) -> Result<StagedArtifact, ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn publish(&self, _staged: &StagedArtifact) -> Result<PublishState, ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn reconcile_publish(
        &self,
        _staged: &StagedArtifact,
    ) -> Result<PublishState, ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn verify(
        &self,
        _key: ArtifactKey,
        _expected_size: u64,
        _expected_sha256: Sha256Digest,
    ) -> Result<VerifiedArtifact, ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn cleanup_verified(&self, _artifact: &VerifiedArtifact) -> Result<(), ArtifactStoreError> {
        Err(ArtifactStoreError)
    }

    pub fn cleanup_staged(&self, _artifact: &StagedArtifact) -> Result<(), ArtifactStoreError> {
        Err(ArtifactStoreError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_adapter_never_claims_a_filesystem_store() {
        let error = UnixArtifactStore::open(Path::new("/not-opened"))
            .expect_err("unsupported adapter must fail before filesystem access");
        assert_eq!(
            error.reason_code(),
            GuardedUndoReasonCode::AdapterUnsupported
        );
    }
}
