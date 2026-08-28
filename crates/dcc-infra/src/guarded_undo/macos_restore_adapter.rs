//! macOS authority adapter for Guarded Undo prepare/execute/recovery.
//!
//! Every authority retains the descriptor-rooted workspace, linked-worktree
//! Git roots, artifact-store binding and coordinator lease for its lifetime.

#![cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]

use std::{path::Path, sync::Arc};

use dcc_core::domain::guarded_undo::{
    ArtifactKey, CheckoutRefV1, GitIdentityV1, GuardedUndoReasonCode, IndexIdentityV1,
    OpaqueRepoPath, PhysicalRootId, Sha256Digest, TurnRestoreFile, GIT_IDENTITY_SCHEMA_VERSION,
    MAX_PREIMAGE_BYTES_PER_FILE,
};

use super::{
    coordinator::{
        CaptureEdgeGuard, CoordinatorError, MultiMutationGuard, WorkspaceMutationCoordinator,
    },
    git_inspector::{CheckoutRef, GitInspector, GitInspectorLimits, TrustedGitBinary},
    macos_git_bridge::{MacGitBridgeError, MacGitMutationAuthority},
    macos_root::{IoErrorCategory, MacWorkspaceRoot, MacWorkspaceRootError, VerifiedRegularFile},
    macos_store::{MacArtifactStore, MacArtifactStoreLease},
    restore_service::{
        AuthorityGenerations, AuthorityMode, FileEvidence, InversePreview, RestoreAuthority,
        RestoreAuthorityAdapter,
    },
};

pub(crate) struct MacRestoreAuthorityAdapter {
    store_lease: Arc<MacArtifactStoreLease>,
    coordinator: Arc<WorkspaceMutationCoordinator>,
    git: TrustedGitBinary,
}

impl MacRestoreAuthorityAdapter {
    pub(crate) fn new(
        store_lease: Arc<MacArtifactStoreLease>,
        coordinator: Arc<WorkspaceMutationCoordinator>,
        git: TrustedGitBinary,
    ) -> Self {
        Self {
            store_lease,
            coordinator,
            git,
        }
    }
}

enum AuthorityLease {
    Shared { _guard: CaptureEdgeGuard },
    Exclusive { _guard: MultiMutationGuard },
}

struct MacRestoreAuthority {
    mode: AuthorityMode,
    root: MacWorkspaceRoot,
    git_authority: MacGitMutationAuthority,
    store: MacArtifactStore,
    generations: AuthorityGenerations,
    git: TrustedGitBinary,
    _lease: AuthorityLease,
}

impl RestoreAuthorityAdapter for MacRestoreAuthorityAdapter {
    fn acquire(
        &self,
        workspace_absolute: &Path,
        expected_root: &PhysicalRootId,
        mode: AuthorityMode,
    ) -> Result<Box<dyn RestoreAuthority>, GuardedUndoReasonCode> {
        if !workspace_absolute.is_absolute() {
            return Err(GuardedUndoReasonCode::WorkspaceMissing);
        }
        let root = MacWorkspaceRoot::open_absolute(workspace_absolute).map_err(root_reason)?;
        if root.physical_root_id() != *expected_root {
            return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        let git_authority =
            MacGitMutationAuthority::open(workspace_absolute).map_err(bridge_reason)?;
        if git_authority.worktree_root_id() != *expected_root {
            return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        let git_dir_id = git_authority.git_dir_id();
        let common_dir_id = git_authority.common_dir_id();
        let roots = vec![
            expected_root.clone(),
            git_dir_id.clone(),
            common_dir_id.clone(),
        ];
        let lease = match mode {
            AuthorityMode::Shared => AuthorityLease::Shared {
                _guard: self
                    .coordinator
                    .try_acquire_capture_edges(roots)
                    .map_err(coordinator_reason)?,
            },
            AuthorityMode::Exclusive => AuthorityLease::Exclusive {
                _guard: self
                    .coordinator
                    .try_acquire_mutations(roots)
                    .map_err(coordinator_reason)?,
            },
        };

        // Revalidate after admission. Discovery before the coordinator lease
        // is only a hint and can never authorize filesystem mutation.
        root.validate_root_directory().map_err(root_reason)?;
        git_authority.revalidate().map_err(bridge_reason)?;
        if root.physical_root_id() != *expected_root
            || git_authority.worktree_root_id() != *expected_root
        {
            return Err(GuardedUndoReasonCode::RepositoryIdentityChanged);
        }
        let generations = AuthorityGenerations {
            worktree: self
                .coordinator
                .generation(expected_root)
                .map_err(coordinator_reason)?,
            git_dir: self
                .coordinator
                .generation(&git_dir_id)
                .map_err(coordinator_reason)?,
            common_dir: self
                .coordinator
                .generation(&common_dir_id)
                .map_err(coordinator_reason)?,
        };
        let store = self
            .store_lease
            .bind_workspace(&root)
            .map_err(|error| error.reason_code())?;
        Ok(Box::new(MacRestoreAuthority {
            mode,
            root,
            git_authority,
            store,
            generations,
            git: self.git.clone(),
            _lease: lease,
        }))
    }
}

impl RestoreAuthority for MacRestoreAuthority {
    fn mode(&self) -> AuthorityMode {
        self.mode
    }

    fn root_id(&self) -> PhysicalRootId {
        self.root.physical_root_id()
    }

    fn git_identity(&self) -> Result<GitIdentityV1, GuardedUndoReasonCode> {
        self.git_authority.revalidate().map_err(bridge_reason)?;
        let inspector = GitInspector::with_index_reader(
            GitInspectorLimits::default(),
            self.git.clone(),
            self.git_authority.index_reader(),
        )
        .map_err(|error| error.reason_code())?;
        let inspection = inspector
            .inspect(self.git_authority.workspace_path())
            .map_err(|error| error.reason_code())?;
        let checkout_ref = match inspection.checkout_ref {
            CheckoutRef::Symbolic { full_name } => CheckoutRefV1::Symbolic {
                full_name: String::from_utf8(full_name)
                    .map_err(|_| GuardedUndoReasonCode::RefChanged)?,
            },
            CheckoutRef::Detached => CheckoutRefV1::Detached,
        };
        let identity = GitIdentityV1 {
            schema_version: GIT_IDENTITY_SCHEMA_VERSION,
            worktree_identity: self.git_authority.worktree_root_id().0,
            git_dir_identity: self.git_authority.git_dir_id().0,
            common_dir_identity: self.git_authority.common_dir_id().0,
            head_oid: inspection.head_oid,
            checkout_ref,
            index: IndexIdentityV1 {
                sha256: inspection.index.sha256,
                size: inspection.index.size,
                stat_identity: inspection.index.stat_identity,
            },
        };
        identity
            .validate()
            .map_err(|_| GuardedUndoReasonCode::InvalidPersistedRecord)?;
        Ok(identity)
    }

    fn coordinator_generations(&self) -> AuthorityGenerations {
        // The captured value belongs to the retained admission lease. Reading
        // the coordinator again would make this proof depend on later state.
        self.generations
    }

    fn inspect_target(&self, path: &OpaqueRepoPath) -> Result<FileEvidence, GuardedUndoReasonCode> {
        self.root
            .observe_regular_file(path, MAX_PREIMAGE_BYTES_PER_FILE)
            .map(evidence)
            .map_err(root_reason)
    }

    fn verify_preimage(
        &self,
        key: ArtifactKey,
        size: u64,
        sha256: Sha256Digest,
    ) -> Result<(), GuardedUndoReasonCode> {
        self.store
            .verify(key, size, sha256)
            .map(|_| ())
            .map_err(|error| error.reason_code())
    }

    fn inverse_preview(
        &self,
        file: &TurnRestoreFile,
    ) -> Result<InversePreview, GuardedUndoReasonCode> {
        let current = self
            .root
            .read_stable_twice(&file.path_bytes, file.result_size, None)
            .map_err(root_reason)?;
        if current.sha256 != file.result_sha256 {
            return Err(GuardedUndoReasonCode::TargetResultMismatch);
        }
        let preimage = self
            .store
            .read_verified(file.pre_artifact_key, file.pre_size, file.pre_sha256)
            .map_err(|error| error.reason_code())?;
        let current_bytes = current.bytes.as_slice();
        let preimage_bytes = preimage.as_slice();
        let binary = is_binary(current_bytes) || is_binary(preimage_bytes);
        Ok(InversePreview {
            display_path: display_path(&file.path_bytes),
            size: file.pre_size,
            binary,
            preview: (!binary).then(|| inverse_text_preview(current_bytes, preimage_bytes)),
        })
    }

    fn stage_preimage(
        &self,
        file: &TurnRestoreFile,
        exchange_key: ArtifactKey,
    ) -> Result<FileEvidence, GuardedUndoReasonCode> {
        let bytes = self
            .store
            .read_verified(file.pre_artifact_key, file.pre_size, file.pre_sha256)
            .map_err(|error| error.reason_code())?;
        self.root
            .stage_exchange_file(
                &file.path_bytes,
                exchange_key,
                &bytes,
                file.pre_sha256,
                &file.metadata_fingerprint,
            )
            .map(|prepared| evidence(prepared.identity))
            .map_err(root_reason)
    }

    fn inspect_exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
    ) -> Result<FileEvidence, GuardedUndoReasonCode> {
        self.root
            .observe_exchange_file(path, exchange_key, MAX_PREIMAGE_BYTES_PER_FILE)
            .map(evidence)
            .map_err(root_reason)
    }

    fn exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
        expected_target: &FileEvidence,
        expected_exchange: &FileEvidence,
    ) -> Result<(), GuardedUndoReasonCode> {
        self.root
            .exchange_target_with_sibling(
                path,
                exchange_key,
                &verified(expected_target),
                &verified(expected_exchange),
            )
            .map(|_| ())
            .map_err(root_reason)
    }

    fn cleanup_exchange(
        &self,
        path: &OpaqueRepoPath,
        exchange_key: ArtifactKey,
        expected_exchange: &FileEvidence,
    ) -> Result<(), GuardedUndoReasonCode> {
        self.root
            .cleanup_exchange_file(path, exchange_key, &verified(expected_exchange))
            .map_err(root_reason)
    }
}

fn evidence(file: VerifiedRegularFile) -> FileEvidence {
    FileEvidence {
        size: file.size,
        sha256: file.sha256,
        metadata: file.metadata,
    }
}

fn verified(file: &FileEvidence) -> VerifiedRegularFile {
    VerifiedRegularFile {
        size: file.size,
        sha256: file.sha256,
        metadata: file.metadata.clone(),
    }
}

fn coordinator_reason(error: CoordinatorError) -> GuardedUndoReasonCode {
    match error {
        CoordinatorError::MutationInProgress | CoordinatorError::CaptureEdgeActive => {
            GuardedUndoReasonCode::MutationInProgress
        }
        _ => GuardedUndoReasonCode::ConcurrentWorkspaceMutation,
    }
}

fn root_reason(error: MacWorkspaceRootError) -> GuardedUndoReasonCode {
    match error {
        MacWorkspaceRootError::Io(IoErrorCategory::NotFound) => {
            GuardedUndoReasonCode::TargetMissing
        }
        MacWorkspaceRootError::Io(IoErrorCategory::PermissionDenied) => {
            GuardedUndoReasonCode::PermissionDenied
        }
        other => other.reason_code(),
    }
}

fn bridge_reason(error: MacGitBridgeError) -> GuardedUndoReasonCode {
    match error {
        MacGitBridgeError::IndexTooLarge => GuardedUndoReasonCode::IndexTooLarge,
        MacGitBridgeError::IndexChanged => GuardedUndoReasonCode::CaptureRace,
        MacGitBridgeError::IndexUnreadable => GuardedUndoReasonCode::IndexUnreadable,
        MacGitBridgeError::InvalidWorkspace
        | MacGitBridgeError::LayoutMismatch
        | MacGitBridgeError::LayoutEscape
        | MacGitBridgeError::UnsafeGitMetadata
        | MacGitBridgeError::UnsupportedLayout => GuardedUndoReasonCode::RepositoryIdentityChanged,
    }
}

fn display_path(path: &OpaqueRepoPath) -> String {
    if path.validate().is_err() || path.as_persisted_bytes().len() < 3 {
        return "[invalid-path]".to_owned();
    }
    let bytes = &path.as_persisted_bytes()[2..];
    let mut output = String::new();
    for &byte in bytes {
        if byte.is_ascii_graphic() || byte == b' ' {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("\\x{byte:02x}"));
        }
    }
    output
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

fn inverse_text_preview(current: &[u8], preimage: &[u8]) -> String {
    const MAX_PREVIEW_BYTES: usize = 64 * 1024;
    let current = std::str::from_utf8(current).unwrap_or_default();
    let preimage = std::str::from_utf8(preimage).unwrap_or_default();
    let mut preview = String::from("--- current\n+++ restored\n");
    for line in current.lines().take(200) {
        preview.push('-');
        preview.push_str(line);
        preview.push('\n');
        if preview.len() >= MAX_PREVIEW_BYTES {
            break;
        }
    }
    if preview.len() < MAX_PREVIEW_BYTES {
        for line in preimage.lines().take(200) {
            preview.push('+');
            preview.push_str(line);
            preview.push('\n');
            if preview.len() >= MAX_PREVIEW_BYTES {
                break;
            }
        }
    }
    preview.truncate(preview.len().min(MAX_PREVIEW_BYTES));
    preview
}
