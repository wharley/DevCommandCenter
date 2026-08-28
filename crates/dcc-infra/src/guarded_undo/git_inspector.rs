//! Bounded, read-only Git inspection for Guarded Undo capture-v2.
//!
//! The inspector deliberately does not read worktree file contents. The only
//! direct file access is the active index, behind [`IndexFileReader`]. Git is
//! invoked only through a closed set of read-only builtins; hooks, filters,
//! diff/textconv drivers, editors, pagers, credential helpers and fsmonitor
//! are never part of the inspection protocol.

#[cfg(any(target_os = "linux", target_os = "macos", test))]
use std::io::Read;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::Write;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::{Command, Stdio};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
    process::ExitStatus,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
use std::{
    fs::{File, OpenOptions},
    sync::Arc,
};

use dcc_core::domain::guarded_undo::{
    GuardedUndoReasonCode, OpaqueRepoPath, Sha256Digest, MAX_BASELINE_FILES, MAX_INDEX_BYTES,
    MAX_OPAQUE_PATH_BYTES,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_COMMANDS: usize = 16;
const MAX_STDOUT_BYTES: usize = 96 * 1024 * 1024;
#[cfg(any(target_os = "linux", target_os = "macos"))]
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_TOTAL_PATH_BYTES: usize = 16 * 1024 * 1024;
const MAX_REF_BYTES: usize = 1024;
const MAX_LAYOUT_PATH_BYTES: usize = 16 * 1024;
const MAX_DEADLINE: Duration = Duration::from_secs(30);
const ATTRIBUTE_NAMES: [&str; 6] = [
    "filter",
    "working-tree-encoding",
    "ident",
    "text",
    "eol",
    "crlf",
];

#[derive(Clone, Debug)]
pub struct GitInspectorLimits {
    pub deadline: Duration,
    pub max_tracked_entries: usize,
    pub max_untracked_paths: usize,
    pub max_total_path_bytes: usize,
    pub max_index_bytes: u64,
    pub max_command_output_bytes: usize,
}

impl Default for GitInspectorLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(10),
            max_tracked_entries: MAX_BASELINE_FILES as usize,
            max_untracked_paths: MAX_BASELINE_FILES as usize,
            max_total_path_bytes: MAX_TOTAL_PATH_BYTES,
            max_index_bytes: MAX_INDEX_BYTES,
            max_command_output_bytes: MAX_STDOUT_BYTES,
        }
    }
}

impl GitInspectorLimits {
    fn validate(&self) -> Result<(), GitInspectorError> {
        if self.deadline.is_zero()
            || self.deadline > MAX_DEADLINE
            || self.max_tracked_entries == 0
            || self.max_tracked_entries > MAX_BASELINE_FILES as usize
            || self.max_untracked_paths > MAX_BASELINE_FILES as usize
            || self.max_total_path_bytes == 0
            || self.max_total_path_bytes > MAX_TOTAL_PATH_BYTES
            || self.max_index_bytes == 0
            || self.max_index_bytes > MAX_INDEX_BYTES
            || self.max_command_output_bytes == 0
            || self.max_command_output_bytes > MAX_STDOUT_BYTES
        {
            return Err(GitInspectorError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitInspectionStep {
    BareRepository,
    GitDirectory,
    CommonDirectory,
    Worktree,
    Head,
    CheckoutRef,
    IndexPath,
    SharedIndex,
    TrackedManifest,
    UntrackedPaths,
    Attributes,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GitInspectorError {
    #[error("guarded undo Git inspector limits are invalid")]
    InvalidLimits,
    #[error("guarded undo Git inspection is unsupported for a bare repository")]
    BareRepository,
    #[error("guarded undo Git inspection timed out")]
    Timeout,
    #[error("guarded undo Git command output exceeded its bound at {step:?}")]
    OutputLimit { step: GitInspectionStep },
    #[error("guarded undo Git command failed at {step:?}")]
    CommandFailed { step: GitInspectionStep },
    #[error("guarded undo Git output was invalid at {step:?}")]
    InvalidOutput { step: GitInspectionStep },
    #[error("guarded undo Git inspection exceeded a path or entry bound")]
    ManifestLimit,
    #[error("guarded undo index could not be observed")]
    IndexUnreadable,
    #[error("guarded undo index exceeded its byte bound")]
    IndexTooLarge,
    #[error("guarded undo index changed while it was inspected")]
    IndexChangedDuringInspection,
    #[error("guarded undo Git inspector is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("guarded undo Git binary was not trusted")]
    UntrustedGitBinary,
    #[error("guarded undo trusted Git binary identity changed")]
    GitBinaryChanged,
}

impl GitInspectorError {
    pub fn reason_code(&self) -> GuardedUndoReasonCode {
        match self {
            Self::BareRepository => GuardedUndoReasonCode::BareRepository,
            Self::Timeout => GuardedUndoReasonCode::CaptureTimeout,
            Self::ManifestLimit | Self::OutputLimit { .. } => {
                GuardedUndoReasonCode::TooManyBaselineFiles
            }
            Self::IndexUnreadable => GuardedUndoReasonCode::IndexUnreadable,
            Self::IndexTooLarge => GuardedUndoReasonCode::IndexTooLarge,
            Self::IndexChangedDuringInspection => GuardedUndoReasonCode::CaptureRace,
            Self::UnsupportedPlatform => GuardedUndoReasonCode::AdapterUnsupported,
            Self::UntrustedGitBinary | Self::GitBinaryChanged => {
                GuardedUndoReasonCode::AdapterUnsupported
            }
            Self::InvalidLimits | Self::CommandFailed { .. } | Self::InvalidOutput { .. } => {
                GuardedUndoReasonCode::IoError
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct UntrustedGitPath(Vec<u8>);

impl UntrustedGitPath {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[cfg(test)]
    pub(crate) fn from_test_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for UntrustedGitPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UntrustedGitPath([redacted])")
    }
}

/// Absolute executable selected by the caller and bound to its Unix physical
/// identity. This token does not come from the inspected repository or PATH.
#[derive(Clone)]
pub struct TrustedGitBinary {
    #[cfg(target_os = "linux")]
    executable: Arc<File>,
    #[cfg(target_os = "linux")]
    identity: Vec<u8>,
    #[cfg(target_os = "macos")]
    system_path: PathBuf,
    #[cfg(target_os = "macos")]
    identity: Vec<u8>,
}

impl fmt::Debug for TrustedGitBinary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TrustedGitBinary([redacted])")
    }
}

impl TrustedGitBinary {
    pub fn verify_absolute(path: &Path) -> Result<Self, GitInspectorError> {
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = path;
            Err(GitInspectorError::UnsupportedPlatform)
        }
        #[cfg(target_os = "linux")]
        {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err(GitInspectorError::UntrustedGitBinary);
            }
            validate_no_symlink_components(path)?;
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            let executable = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(path)
                .map_err(|_| GitInspectorError::UntrustedGitBinary)?;
            let metadata = executable
                .metadata()
                .map_err(|_| GitInspectorError::UntrustedGitBinary)?;
            let effective_uid = unsafe { libc::geteuid() };
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || (metadata.uid() != 0 && metadata.uid() != effective_uid)
                || metadata.mode() & 0o111 == 0
                || metadata.mode() & 0o022 != 0
            {
                return Err(GitInspectorError::UntrustedGitBinary);
            }
            Ok(Self {
                executable: Arc::new(executable),
                identity: trusted_binary_identity(&metadata),
            })
        }
        #[cfg(target_os = "macos")]
        {
            if path != Path::new("/usr/bin/git") {
                return Err(GitInspectorError::UnsupportedPlatform);
            }
            let metadata = validate_macos_system_git_path(path)?;
            Ok(Self {
                system_path: path.to_path_buf(),
                identity: trusted_binary_identity(&metadata),
            })
        }
    }

    /// Discovers only the Git directories needed to coordinate a mutation.
    ///
    /// This deliberately uses the same verified binary, scrubbed environment,
    /// bounded output, process-group timeout, and closed read-only builtin set
    /// as the full capture inspector. Returned paths remain untrusted until a
    /// platform adapter binds them to retained physical directory handles.
    pub(crate) fn discover_mutation_layout(
        &self,
        workspace: &Path,
    ) -> Result<GitMutationLayout, GitInspectorError> {
        let started = Instant::now();
        let bare = mutation_discovery_text(
            self,
            workspace,
            GitInspectionStep::BareRepository,
            &["--is-bare-repository"],
            started,
        )?;
        match bare.as_slice() {
            b"false" => {}
            b"true" => return Err(GitInspectorError::BareRepository),
            _ => {
                return Err(GitInspectorError::InvalidOutput {
                    step: GitInspectionStep::BareRepository,
                })
            }
        }

        let git_dir = mutation_discovery_path(
            self,
            workspace,
            GitInspectionStep::GitDirectory,
            &["--path-format=absolute", "--git-dir"],
            started,
        )?;
        let common_dir = mutation_discovery_path(
            self,
            workspace,
            GitInspectionStep::CommonDirectory,
            &["--path-format=absolute", "--git-common-dir"],
            started,
        )?;
        let worktree = mutation_discovery_path(
            self,
            workspace,
            GitInspectionStep::Worktree,
            &["--path-format=absolute", "--show-toplevel"],
            started,
        )?;
        Ok(GitMutationLayout {
            git_dir,
            common_dir,
            worktree,
        })
    }

    #[cfg(target_os = "linux")]
    fn revalidate(&self) -> Result<(), GitInspectorError> {
        let metadata = self
            .executable
            .metadata()
            .map_err(|_| GitInspectorError::GitBinaryChanged)?;
        if !metadata.file_type().is_file() || trusted_binary_identity(&metadata) != self.identity {
            return Err(GitInspectorError::GitBinaryChanged);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn revalidate(&self) -> Result<(), GitInspectorError> {
        let metadata = validate_macos_system_git_path(&self.system_path)
            .map_err(|_| GitInspectorError::GitBinaryChanged)?;
        if !macos_identity_matches(&self.identity, &trusted_binary_identity(&metadata)) {
            return Err(GitInspectorError::GitBinaryChanged);
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn executable_fd_path(&self) -> PathBuf {
        use std::os::fd::AsRawFd;
        Path::new("/proc/self/fd").join(self.executable.as_raw_fd().to_string())
    }
}

/// Logical Git authority paths used only as input to a physical platform
/// binding. No caller may treat these path strings as mutation authority.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GitMutationLayout {
    pub(crate) git_dir: UntrustedGitPath,
    pub(crate) common_dir: UntrustedGitPath,
    pub(crate) worktree: UntrustedGitPath,
}

impl fmt::Debug for GitMutationLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitMutationLayout([redacted])")
    }
}

#[cfg(target_os = "linux")]
fn validate_no_symlink_components(path: &Path) -> Result<(), GitInspectorError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| GitInspectorError::UntrustedGitBinary)?;
        if metadata.file_type().is_symlink() {
            return Err(GitInspectorError::UntrustedGitBinary);
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_macos_system_git_path(path: &Path) -> Result<std::fs::Metadata, GitInspectorError> {
    use std::os::unix::fs::MetadataExt;
    let mut current = PathBuf::new();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| GitInspectorError::UntrustedGitBinary)?;
        let final_component = index + 1 == components.len();
        if metadata.file_type().is_symlink()
            || !macos_system_component_is_safe(
                metadata.uid(),
                metadata.mode(),
                metadata.is_dir(),
                metadata.is_file(),
                final_component,
            )
        {
            return Err(GitInspectorError::UntrustedGitBinary);
        }
        if final_component {
            return Ok(metadata);
        }
    }
    Err(GitInspectorError::UntrustedGitBinary)
}

#[cfg(any(target_os = "macos", test))]
fn macos_system_component_is_safe(
    uid: u32,
    mode: u32,
    is_directory: bool,
    is_regular: bool,
    final_component: bool,
) -> bool {
    if uid != 0 || mode & 0o022 != 0 {
        return false;
    }
    if final_component {
        is_regular && mode & 0o111 != 0
    } else {
        is_directory
    }
}

#[cfg(any(target_os = "macos", test))]
fn macos_identity_matches(expected: &[u8], observed: &[u8]) -> bool {
    expected == observed
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn trusted_binary_identity(metadata: &std::fs::Metadata) -> Vec<u8> {
    use std::os::unix::fs::MetadataExt;
    let fields = [
        metadata.dev(),
        metadata.ino(),
        metadata.mode() as u64,
        metadata.uid() as u64,
        metadata.gid() as u64,
        metadata.size(),
        metadata.mtime() as u64,
        metadata.mtime_nsec() as u64,
        metadata.ctime() as u64,
        metadata.ctime_nsec() as u64,
    ];
    let mut result = Vec::with_capacity(fields.len() * 8);
    for field in fields {
        result.extend_from_slice(&field.to_le_bytes());
    }
    result
}

#[derive(Clone, PartialEq, Eq)]
pub enum CheckoutRef {
    Symbolic { full_name: Vec<u8> },
    Detached,
}

impl fmt::Debug for CheckoutRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbolic { .. } => formatter.write_str("CheckoutRef::Symbolic([redacted])"),
            Self::Detached => formatter.write_str("CheckoutRef::Detached"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackedPathKind {
    Regular,
    Symlink,
    Submodule,
    SparseDirectory,
    Unmerged,
    UnsupportedMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributeHazard {
    Filter,
    WorkingTreeEncoding,
    Ident,
    Text,
    Eol,
    Crlf,
}

impl AttributeHazard {
    pub fn reason_code(self) -> GuardedUndoReasonCode {
        match self {
            Self::WorkingTreeEncoding => GuardedUndoReasonCode::WorkingTreeEncodingPresent,
            Self::Filter | Self::Ident | Self::Text | Self::Eol | Self::Crlf => {
                GuardedUndoReasonCode::GitFilterPresent
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TrackedManifestEntry {
    pub path: OpaqueRepoPath,
    pub mode: u32,
    pub stage: u8,
    pub object_id: Vec<u8>,
    pub assume_unchanged: bool,
    pub skip_worktree: bool,
    pub kind: TrackedPathKind,
    pub attribute_hazards: BTreeSet<AttributeHazard>,
}

impl fmt::Debug for TrackedManifestEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrackedManifestEntry")
            .field("path", &"[redacted]")
            .field("mode", &self.mode)
            .field("stage", &self.stage)
            .field("object_id", &"[redacted]")
            .field("assume_unchanged", &self.assume_unchanged)
            .field("skip_worktree", &self.skip_worktree)
            .field("kind", &self.kind)
            .field("attribute_hazards", &self.attribute_hazards)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct IndexObservation {
    pub sha256: Sha256Digest,
    pub size: u64,
    pub stat_identity: Vec<u8>,
}

impl fmt::Debug for IndexObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexObservation")
            .field("size", &self.size)
            .field("identity", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct UntrustedGitLayout {
    /// Logical paths emitted by Git. They are untrusted until a platform
    /// adapter resolves them relative to its held physical root handles.
    pub git_dir: UntrustedGitPath,
    pub common_dir: UntrustedGitPath,
    pub worktree: UntrustedGitPath,
    pub active_index_path: UntrustedGitPath,
}

impl fmt::Debug for UntrustedGitLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UntrustedGitLayout")
            .field("paths", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct GitInspection {
    pub layout: UntrustedGitLayout,
    pub head_oid: Vec<u8>,
    pub checkout_ref: CheckoutRef,
    pub index: IndexObservation,
    pub split_index: bool,
    pub tracked: Vec<TrackedManifestEntry>,
    pub tracked_manifest_sha256: Sha256Digest,
    pub attributes_sha256: Sha256Digest,
    pub untracked: Vec<OpaqueRepoPath>,
    /// Logical classifications only. An empty vector MUST NOT be interpreted
    /// as eligible until the platform adapter validates every physical path.
    pub logical_ineligibility_reasons: Vec<GuardedUndoReasonCode>,
    pub physical_path_validation: PhysicalPathValidation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalPathValidation {
    Required,
}

impl fmt::Debug for GitInspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitInspection")
            .field("layout", &"[redacted]")
            .field("head", &"[redacted]")
            .field("checkout_ref", &"[redacted]")
            .field("index", &self.index)
            .field("split_index", &self.split_index)
            .field("tracked_entries", &self.tracked.len())
            .field("untracked_paths", &self.untracked.len())
            .field(
                "logical_ineligibility_reasons",
                &self.logical_ineligibility_reasons,
            )
            .field("physical_path_validation", &self.physical_path_validation)
            .finish()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IndexReadError {
    #[error("active index is unreadable")]
    Unreadable,
    #[error("active index exceeds its byte bound")]
    TooLarge,
    #[error("active index changed during one observation")]
    Changed,
    #[error("active index identity is unsupported")]
    Unsupported,
}

/// Port that confines the inspector's only direct filesystem read.
pub trait IndexFileReader: Send + Sync {
    /// All paths are Git-controlled and MUST be resolved no-follow by the
    /// platform adapter before reading. Implementations own that validation.
    fn observe(
        &self,
        path: &UntrustedGitPath,
        layout: &UntrustedGitLayout,
        maximum_bytes: u64,
    ) -> Result<IndexObservation, IndexReadError>;
}

pub struct GitInspector<R> {
    index_reader: R,
    limits: GitInspectorLimits,
    git_binary: TrustedGitBinary,
}

impl<R: IndexFileReader> GitInspector<R> {
    pub fn with_index_reader(
        limits: GitInspectorLimits,
        git_binary: TrustedGitBinary,
        index_reader: R,
    ) -> Result<Self, GitInspectorError> {
        limits.validate()?;
        Ok(Self {
            index_reader,
            limits,
            git_binary,
        })
    }

    pub fn inspect(&self, workspace: &Path) -> Result<GitInspection, GitInspectorError> {
        let started = Instant::now();
        let mut command_count = 0_usize;

        let bare = self.run_text(
            workspace,
            GitBuiltin::RevParse,
            GitInspectionStep::BareRepository,
            &["--is-bare-repository"],
            &[],
            started,
            &mut command_count,
        )?;
        match bare.as_slice() {
            b"true" => return Err(GitInspectorError::BareRepository),
            b"false" => {}
            _ => {
                return Err(GitInspectorError::InvalidOutput {
                    step: GitInspectionStep::BareRepository,
                })
            }
        }

        let git_dir = self.resolve_path(
            workspace,
            GitInspectionStep::GitDirectory,
            &["--path-format=absolute", "--git-dir"],
            started,
            &mut command_count,
        )?;
        let common_dir = self.resolve_path(
            workspace,
            GitInspectionStep::CommonDirectory,
            &["--path-format=absolute", "--git-common-dir"],
            started,
            &mut command_count,
        )?;
        let worktree = self.resolve_path(
            workspace,
            GitInspectionStep::Worktree,
            &["--path-format=absolute", "--show-toplevel"],
            started,
            &mut command_count,
        )?;
        let head_hex = self.run_text(
            workspace,
            GitBuiltin::RevParse,
            GitInspectionStep::Head,
            &["--verify", "HEAD"],
            &[],
            started,
            &mut command_count,
        )?;
        let head_oid = decode_oid(&head_hex).ok_or(GitInspectorError::InvalidOutput {
            step: GitInspectionStep::Head,
        })?;
        let checkout_ref = self.symbolic_ref(workspace, started, &mut command_count)?;
        let index_path_raw = self.resolve_path(
            workspace,
            GitInspectionStep::IndexPath,
            &["--path-format=absolute", "--git-path", "index"],
            started,
            &mut command_count,
        )?;
        let layout = UntrustedGitLayout {
            git_dir,
            common_dir,
            worktree,
            active_index_path: index_path_raw,
        };
        let first_index = self
            .index_reader
            .observe(
                &layout.active_index_path,
                &layout,
                self.limits.max_index_bytes,
            )
            .map_err(map_index_read_error)?;

        let shared_index = self.run_text(
            workspace,
            GitBuiltin::RevParse,
            GitInspectionStep::SharedIndex,
            &["--path-format=absolute", "--shared-index-path"],
            &[],
            started,
            &mut command_count,
        )?;
        let split_index = !shared_index.is_empty();
        if split_index {
            validate_layout_path(&shared_index, GitInspectionStep::SharedIndex)?;
        }

        let tracked_output = self.run_nul(
            workspace,
            GitBuiltin::LsFiles,
            GitInspectionStep::TrackedManifest,
            &["--stage", "-v", "--sparse", "-z"],
            &[],
            started,
            &mut command_count,
        )?;
        let mut tracked = parse_tracked_manifest(&tracked_output, &self.limits)?;

        let untracked_output = self.run_nul(
            workspace,
            GitBuiltin::LsFiles,
            GitInspectionStep::UntrackedPaths,
            &["--others", "--exclude-standard", "-z"],
            &[],
            started,
            &mut command_count,
        )?;
        let untracked = parse_untracked(&untracked_output, &self.limits)?;

        let attributes_sha256 =
            attach_attributes(self, workspace, &mut tracked, started, &mut command_count)?;

        let final_index_path = self.resolve_path(
            workspace,
            GitInspectionStep::IndexPath,
            &["--path-format=absolute", "--git-path", "index"],
            started,
            &mut command_count,
        )?;
        if final_index_path != layout.active_index_path {
            return Err(GitInspectorError::IndexChangedDuringInspection);
        }
        let second_index = self
            .index_reader
            .observe(
                &layout.active_index_path,
                &layout,
                self.limits.max_index_bytes,
            )
            .map_err(map_index_read_error)?;
        if first_index != second_index {
            return Err(GitInspectorError::IndexChangedDuringInspection);
        }

        let tracked_manifest_sha256 = manifest_digest(&tracked);
        let logical_ineligibility_reasons =
            inspection_reasons(&checkout_ref, split_index, &tracked, &untracked);
        Ok(GitInspection {
            layout,
            head_oid,
            checkout_ref,
            index: second_index,
            split_index,
            tracked,
            tracked_manifest_sha256,
            attributes_sha256,
            untracked,
            logical_ineligibility_reasons,
            physical_path_validation: PhysicalPathValidation::Required,
        })
    }

    fn symbolic_ref(
        &self,
        workspace: &Path,
        started: Instant,
        command_count: &mut usize,
    ) -> Result<CheckoutRef, GitInspectorError> {
        let output = self.run(
            workspace,
            GitBuiltin::SymbolicRef,
            GitInspectionStep::CheckoutRef,
            &["--quiet", "HEAD"],
            &[],
            started,
            command_count,
        )?;
        if output.status.success() {
            let full_name = single_line(&output.stdout, GitInspectionStep::CheckoutRef)?;
            if full_name.len() > MAX_REF_BYTES || !full_name.starts_with(b"refs/") {
                return Err(GitInspectorError::InvalidOutput {
                    step: GitInspectionStep::CheckoutRef,
                });
            }
            Ok(CheckoutRef::Symbolic { full_name })
        } else if output.status.code() == Some(1) {
            Ok(CheckoutRef::Detached)
        } else {
            Err(GitInspectorError::CommandFailed {
                step: GitInspectionStep::CheckoutRef,
            })
        }
    }

    fn resolve_path(
        &self,
        workspace: &Path,
        step: GitInspectionStep,
        args: &[&str],
        started: Instant,
        command_count: &mut usize,
    ) -> Result<UntrustedGitPath, GitInspectorError> {
        let raw = self.run_text(
            workspace,
            GitBuiltin::RevParse,
            step,
            args,
            &[],
            started,
            command_count,
        )?;
        validate_layout_path(&raw, step)?;
        Ok(UntrustedGitPath(raw))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_text(
        &self,
        workspace: &Path,
        builtin: GitBuiltin,
        step: GitInspectionStep,
        args: &[&str],
        stdin: &[u8],
        started: Instant,
        command_count: &mut usize,
    ) -> Result<Vec<u8>, GitInspectorError> {
        let output = self.run(
            workspace,
            builtin,
            step,
            args,
            stdin,
            started,
            command_count,
        )?;
        if !output.status.success() {
            return Err(GitInspectorError::CommandFailed { step });
        }
        single_line(&output.stdout, step)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_nul(
        &self,
        workspace: &Path,
        builtin: GitBuiltin,
        step: GitInspectionStep,
        args: &[&str],
        stdin: &[u8],
        started: Instant,
        command_count: &mut usize,
    ) -> Result<Vec<u8>, GitInspectorError> {
        let output = self.run(
            workspace,
            builtin,
            step,
            args,
            stdin,
            started,
            command_count,
        )?;
        if !output.status.success() {
            return Err(GitInspectorError::CommandFailed { step });
        }
        if !output.stdout.is_empty() && !output.stdout.ends_with(&[0]) {
            return Err(GitInspectorError::InvalidOutput { step });
        }
        Ok(output.stdout)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        workspace: &Path,
        builtin: GitBuiltin,
        step: GitInspectionStep,
        args: &[&str],
        stdin_bytes: &[u8],
        started: Instant,
        command_count: &mut usize,
    ) -> Result<BoundedOutput, GitInspectorError> {
        *command_count = command_count
            .checked_add(1)
            .ok_or(GitInspectorError::InvalidLimits)?;
        if *command_count > MAX_COMMANDS {
            return Err(GitInspectorError::InvalidLimits);
        }
        let remaining = self
            .limits
            .deadline
            .checked_sub(started.elapsed())
            .ok_or(GitInspectorError::Timeout)?;
        run_bounded_git(
            &self.git_binary,
            workspace,
            builtin,
            args,
            stdin_bytes,
            remaining,
            self.limits.max_command_output_bytes,
            step,
        )
    }
}

#[derive(Clone, Copy)]
enum GitBuiltin {
    RevParse,
    SymbolicRef,
    LsFiles,
    CheckAttr,
}

impl GitBuiltin {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn name(self) -> &'static str {
        match self {
            Self::RevParse => "rev-parse",
            Self::SymbolicRef => "symbolic-ref",
            Self::LsFiles => "ls-files",
            Self::CheckAttr => "check-attr",
        }
    }
}

struct BoundedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_bounded_git(
    git_binary: &TrustedGitBinary,
    workspace: &Path,
    builtin: GitBuiltin,
    args: &[&str],
    stdin_bytes: &[u8],
    timeout: Duration,
    stdout_limit: usize,
    step: GitInspectionStep,
) -> Result<BoundedOutput, GitInspectorError> {
    git_binary.revalidate()?;
    #[cfg(target_os = "linux")]
    let executable = git_binary.executable_fd_path();
    #[cfg(target_os = "macos")]
    let executable = git_binary.system_path.clone();
    let mut command = Command::new(executable);
    command
        .env_clear()
        .current_dir(workspace)
        .arg("--no-pager")
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "credential.helper=",
            "-c",
            "core.askPass=",
            "-c",
            "core.pager=cat",
            "-c",
            "pager.status=false",
            "-c",
            "diff.external=",
            "-c",
            "diff.trustExitCode=false",
            "-c",
            "filter.allowRemote=false",
            "-c",
            "core.attributesFile=/dev/null",
        ])
        .arg(builtin.name())
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("HOME", "/dev/null")
        .env("XDG_CONFIG_HOME", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        #[cfg(target_os = "linux")]
        {
            // Force fork/exec so the validated CLOEXEC descriptor remains
            // present while execve resolves /proc/self/fd/N.
            unsafe {
                command.pre_exec(|| Ok(()));
            }
        }
    }

    let mut child = command
        .spawn()
        .map_err(|_| GitInspectorError::UnsupportedPlatform)?;
    let child_id = child.id();
    let child_stdin = child
        .stdin
        .take()
        .ok_or(GitInspectorError::CommandFailed { step })?;
    let mut child_stdout = child
        .stdout
        .take()
        .ok_or(GitInspectorError::CommandFailed { step })?;
    let mut child_stderr = child
        .stderr
        .take()
        .ok_or(GitInspectorError::CommandFailed { step })?;
    if set_nonblocking(&child_stdin).is_err()
        || set_nonblocking(&child_stdout).is_err()
        || set_nonblocking(&child_stderr).is_err()
    {
        kill_child_group(&mut child, child_id);
        return Err(GitInspectorError::CommandFailed { step });
    }

    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(GitInspectorError::Timeout)?;
    let mut input_offset = 0_usize;
    let mut child_stdin = Some(child_stdin);
    let mut stdout_open = true;
    let mut stderr_open = true;
    let mut stdout = Vec::with_capacity(stdout_limit.min(64 * 1024));
    let mut stderr_count = 0_usize;
    let mut status = None;
    loop {
        if let Some(stream) = child_stdin.as_mut() {
            if input_offset == stdin_bytes.len() {
                child_stdin.take();
            } else {
                match stream.write(&stdin_bytes[input_offset..]) {
                    Ok(0) => {
                        kill_child_group(&mut child, child_id);
                        return Err(GitInspectorError::CommandFailed { step });
                    }
                    Ok(count) => input_offset += count,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
                        child_stdin.take();
                    }
                    Err(_) => {
                        kill_child_group(&mut child, child_id);
                        return Err(GitInspectorError::CommandFailed { step });
                    }
                }
            }
        }
        if stdout_open {
            match drain_nonblocking(&mut child_stdout, &mut stdout, stdout_limit) {
                Ok(DrainOutcome::Open) => {}
                Ok(DrainOutcome::Eof) => stdout_open = false,
                Ok(DrainOutcome::Exceeded) => {
                    kill_child_group(&mut child, child_id);
                    return Err(GitInspectorError::OutputLimit { step });
                }
                Err(_) => {
                    kill_child_group(&mut child, child_id);
                    return Err(GitInspectorError::CommandFailed { step });
                }
            }
        }
        if stderr_open {
            match drain_nonblocking_count(&mut child_stderr, &mut stderr_count, MAX_STDERR_BYTES) {
                Ok(DrainOutcome::Open) => {}
                Ok(DrainOutcome::Eof) => stderr_open = false,
                Ok(DrainOutcome::Exceeded) => {
                    kill_child_group(&mut child, child_id);
                    return Err(GitInspectorError::OutputLimit { step });
                }
                Err(_) => {
                    kill_child_group(&mut child, child_id);
                    return Err(GitInspectorError::CommandFailed { step });
                }
            }
        }
        if status.is_none() {
            status = match child.try_wait() {
                Ok(status) => status,
                Err(_) => {
                    kill_child_group(&mut child, child_id);
                    return Err(GitInspectorError::CommandFailed { step });
                }
            };
        }
        if let Some(status) = status {
            if !stdout_open && !stderr_open {
                return Ok(BoundedOutput { status, stdout });
            }
        }
        if Instant::now() >= deadline {
            kill_child_group(&mut child, child_id);
            return Err(GitInspectorError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn mutation_discovery_text(
    git_binary: &TrustedGitBinary,
    workspace: &Path,
    step: GitInspectionStep,
    args: &[&str],
    started: Instant,
) -> Result<Vec<u8>, GitInspectorError> {
    let remaining = Duration::from_secs(10)
        .checked_sub(started.elapsed())
        .ok_or(GitInspectorError::Timeout)?;
    let output = run_bounded_git(
        git_binary,
        workspace,
        GitBuiltin::RevParse,
        args,
        &[],
        remaining,
        MAX_LAYOUT_PATH_BYTES.saturating_add(2),
        step,
    )?;
    if !output.status.success() {
        return Err(GitInspectorError::CommandFailed { step });
    }
    single_line(&output.stdout, step)
}

fn mutation_discovery_path(
    git_binary: &TrustedGitBinary,
    workspace: &Path,
    step: GitInspectionStep,
    args: &[&str],
    started: Instant,
) -> Result<UntrustedGitPath, GitInspectorError> {
    let raw = mutation_discovery_text(git_binary, workspace, step, args, started)?;
    validate_layout_path(&raw, step)?;
    Ok(UntrustedGitPath(raw))
}

#[allow(clippy::too_many_arguments)]
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn run_bounded_git(
    _git_binary: &TrustedGitBinary,
    _workspace: &Path,
    _builtin: GitBuiltin,
    _args: &[&str],
    _stdin_bytes: &[u8],
    _timeout: Duration,
    _stdout_limit: usize,
    _step: GitInspectionStep,
) -> Result<BoundedOutput, GitInspectorError> {
    Err(GitInspectorError::UnsupportedPlatform)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_nonblocking<T: std::os::fd::AsRawFd>(stream: &T) -> std::io::Result<()> {
    let fd = stream.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrainOutcome {
    Open,
    Eof,
    Exceeded,
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn drain_nonblocking<R: Read>(
    reader: &mut R,
    stored: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<DrainOutcome> {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(DrainOutcome::Eof),
            Ok(count) => {
                let remaining = limit.saturating_sub(stored.len());
                if count > remaining {
                    return Ok(DrainOutcome::Exceeded);
                }
                stored.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(DrainOutcome::Open)
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn drain_nonblocking_count<R: Read>(
    reader: &mut R,
    total: &mut usize,
    limit: usize,
) -> std::io::Result<DrainOutcome> {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(DrainOutcome::Eof),
            Ok(count) => {
                let Some(next) = total.checked_add(count) else {
                    return Ok(DrainOutcome::Exceeded);
                };
                if next > limit {
                    return Ok(DrainOutcome::Exceeded);
                }
                *total = next;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(DrainOutcome::Open)
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn kill_child_group(child: &mut std::process::Child, child_id: u32) {
    unsafe {
        libc::kill(-(child_id as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
}

fn single_line(bytes: &[u8], step: GitInspectionStep) -> Result<Vec<u8>, GitInspectorError> {
    let line = bytes
        .strip_suffix(b"\n")
        .unwrap_or(bytes)
        .strip_suffix(b"\r")
        .unwrap_or_else(|| bytes.strip_suffix(b"\n").unwrap_or(bytes));
    if line.contains(&b'\n') || line.contains(&b'\r') || line.contains(&0) {
        return Err(GitInspectorError::InvalidOutput { step });
    }
    Ok(line.to_vec())
}

fn validate_layout_path(bytes: &[u8], step: GitInspectionStep) -> Result<(), GitInspectorError> {
    if bytes.is_empty()
        || bytes.len() > MAX_LAYOUT_PATH_BYTES
        || bytes.contains(&0)
        || bytes.contains(&b'\n')
        || bytes.contains(&b'\r')
    {
        return Err(GitInspectorError::InvalidOutput { step });
    }
    let path = raw_absolute_path(bytes)?;
    if !path.is_absolute() {
        return Err(GitInspectorError::InvalidOutput { step });
    }
    Ok(())
}

fn raw_absolute_path(bytes: &[u8]) -> Result<PathBuf, GitInspectorError> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec()));
        if path.is_absolute() {
            Ok(path)
        } else {
            Err(GitInspectorError::InvalidOutput {
                step: GitInspectionStep::IndexPath,
            })
        }
    }
    #[cfg(not(unix))]
    {
        let _ = bytes;
        Err(GitInspectorError::UnsupportedPlatform)
    }
}

fn decode_oid(hex: &[u8]) -> Option<Vec<u8>> {
    if !matches!(hex.len(), 40 | 64) {
        return None;
    }
    let mut decoded = Vec::with_capacity(hex.len() / 2);
    for pair in hex.chunks_exact(2) {
        let high = decode_hex(pair[0])?;
        let low = decode_hex(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Some(decoded)
}

fn map_index_read_error(error: IndexReadError) -> GitInspectorError {
    match error {
        IndexReadError::Unreadable => GitInspectorError::IndexUnreadable,
        IndexReadError::TooLarge => GitInspectorError::IndexTooLarge,
        IndexReadError::Changed => GitInspectorError::IndexChangedDuringInspection,
        IndexReadError::Unsupported => GitInspectorError::UnsupportedPlatform,
    }
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_tracked_manifest(
    output: &[u8],
    limits: &GitInspectorLimits,
) -> Result<Vec<TrackedManifestEntry>, GitInspectorError> {
    let mut entries = Vec::new();
    let mut total_path_bytes = 0_usize;
    for record in nul_records(output, GitInspectionStep::TrackedManifest)? {
        if entries.len() >= limits.max_tracked_entries {
            return Err(GitInspectorError::ManifestLimit);
        }
        let (metadata, path) =
            split_once(record, b'\t').ok_or(GitInspectorError::InvalidOutput {
                step: GitInspectionStep::TrackedManifest,
            })?;
        if path.is_empty() || path.len() > MAX_OPAQUE_PATH_BYTES.saturating_sub(2) {
            return Err(GitInspectorError::ManifestLimit);
        }
        total_path_bytes = total_path_bytes
            .checked_add(path.len())
            .ok_or(GitInspectorError::ManifestLimit)?;
        if total_path_bytes > limits.max_total_path_bytes {
            return Err(GitInspectorError::ManifestLimit);
        }
        let mut fields = metadata.split(|byte| *byte == b' ');
        let tag = fields.next().filter(|value| value.len() == 1).ok_or(
            GitInspectorError::InvalidOutput {
                step: GitInspectionStep::TrackedManifest,
            },
        )?[0];
        let mode = parse_octal(fields.next(), GitInspectionStep::TrackedManifest)?;
        let object_id = decode_oid(fields.next().unwrap_or_default()).ok_or(
            GitInspectorError::InvalidOutput {
                step: GitInspectionStep::TrackedManifest,
            },
        )?;
        let stage = parse_decimal(fields.next(), GitInspectionStep::TrackedManifest)?;
        if fields.next().is_some() || stage > 3 {
            return Err(GitInspectorError::InvalidOutput {
                step: GitInspectionStep::TrackedManifest,
            });
        }
        let assume_unchanged = tag.is_ascii_lowercase();
        let skip_worktree = tag.eq_ignore_ascii_case(&b'S');
        let kind = classify_entry(mode, stage, skip_worktree);
        entries.push(TrackedManifestEntry {
            path: OpaqueRepoPath::unix(path).map_err(|_| GitInspectorError::InvalidOutput {
                step: GitInspectionStep::TrackedManifest,
            })?,
            mode,
            stage,
            object_id,
            assume_unchanged,
            skip_worktree,
            kind,
            attribute_hazards: BTreeSet::new(),
        });
    }
    entries.sort_by(|left, right| {
        left.path
            .as_persisted_bytes()
            .cmp(right.path.as_persisted_bytes())
            .then(left.stage.cmp(&right.stage))
    });
    Ok(entries)
}

fn parse_untracked(
    output: &[u8],
    limits: &GitInspectorLimits,
) -> Result<Vec<OpaqueRepoPath>, GitInspectorError> {
    let mut paths = Vec::new();
    let mut total = 0_usize;
    for path in nul_records(output, GitInspectionStep::UntrackedPaths)? {
        if paths.len() >= limits.max_untracked_paths
            || path.is_empty()
            || path.len() > MAX_OPAQUE_PATH_BYTES.saturating_sub(2)
        {
            return Err(GitInspectorError::ManifestLimit);
        }
        total = total
            .checked_add(path.len())
            .ok_or(GitInspectorError::ManifestLimit)?;
        if total > limits.max_total_path_bytes {
            return Err(GitInspectorError::ManifestLimit);
        }
        paths.push(
            OpaqueRepoPath::unix(path).map_err(|_| GitInspectorError::InvalidOutput {
                step: GitInspectionStep::UntrackedPaths,
            })?,
        );
    }
    paths.sort_by(|left, right| left.as_persisted_bytes().cmp(right.as_persisted_bytes()));
    paths.dedup();
    Ok(paths)
}

fn attach_attributes<R: IndexFileReader>(
    inspector: &GitInspector<R>,
    workspace: &Path,
    entries: &mut [TrackedManifestEntry],
    started: Instant,
    command_count: &mut usize,
) -> Result<Sha256Digest, GitInspectorError> {
    let mut unique_paths = BTreeSet::new();
    let mut input = Vec::new();
    for entry in entries.iter() {
        let raw = &entry.path.as_persisted_bytes()[2..];
        if unique_paths.insert(raw.to_vec()) {
            input.extend_from_slice(raw);
            input.push(0);
        }
    }
    if input.len() > inspector.limits.max_total_path_bytes + unique_paths.len() {
        return Err(GitInspectorError::ManifestLimit);
    }
    if unique_paths.is_empty() {
        return Ok(Sha256Digest::of(b"DCC_GU_ATTRIBUTES\0\x01\0"));
    }
    let output = inspector.run_nul(
        workspace,
        GitBuiltin::CheckAttr,
        GitInspectionStep::Attributes,
        &[
            "-z",
            "--stdin",
            "filter",
            "working-tree-encoding",
            "ident",
            "text",
            "eol",
            "crlf",
        ],
        &input,
        started,
        command_count,
    )?;
    let fields = nul_records(&output, GitInspectionStep::Attributes)?;
    let expected = unique_paths
        .len()
        .checked_mul(ATTRIBUTE_NAMES.len())
        .and_then(|count| count.checked_mul(3))
        .ok_or(GitInspectorError::ManifestLimit)?;
    if fields.len() != expected {
        return Err(GitInspectorError::InvalidOutput {
            step: GitInspectionStep::Attributes,
        });
    }
    let mut hazards: BTreeMap<Vec<u8>, BTreeSet<AttributeHazard>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut attribute_hasher = Sha256::new();
    attribute_hasher.update(b"DCC_GU_ATTRIBUTES\0\x01");
    attribute_hasher.update((unique_paths.len() as u64).to_le_bytes());
    for triple in fields.chunks_exact(3) {
        let path = triple[0];
        if !unique_paths.contains(path) {
            return Err(GitInspectorError::InvalidOutput {
                step: GitInspectionStep::Attributes,
            });
        }
        let hazard = match triple[1] {
            b"filter" => AttributeHazard::Filter,
            b"working-tree-encoding" => AttributeHazard::WorkingTreeEncoding,
            b"ident" => AttributeHazard::Ident,
            b"text" => AttributeHazard::Text,
            b"eol" => AttributeHazard::Eol,
            b"crlf" => AttributeHazard::Crlf,
            _ => {
                return Err(GitInspectorError::InvalidOutput {
                    step: GitInspectionStep::Attributes,
                })
            }
        };
        if !seen.insert((triple[0].to_vec(), triple[1].to_vec())) {
            return Err(GitInspectorError::InvalidOutput {
                step: GitInspectionStep::Attributes,
            });
        }
        digest_field(&mut attribute_hasher, triple[0]);
        digest_field(&mut attribute_hasher, triple[1]);
        digest_field(&mut attribute_hasher, triple[2]);
        if attribute_is_hazardous(hazard, triple[2]) {
            hazards.entry(path.to_vec()).or_default().insert(hazard);
        }
    }
    for entry in entries {
        if let Some(found) = hazards.get(&entry.path.as_persisted_bytes()[2..]) {
            entry.attribute_hazards = found.clone();
        }
    }
    Ok(Sha256Digest(attribute_hasher.finalize().into()))
}

fn attribute_is_hazardous(hazard: AttributeHazard, value: &[u8]) -> bool {
    if value == b"unspecified" {
        return false;
    }
    match hazard {
        AttributeHazard::Filter | AttributeHazard::WorkingTreeEncoding | AttributeHazard::Ident => {
            value != b"unset"
        }
        AttributeHazard::Text | AttributeHazard::Eol | AttributeHazard::Crlf => true,
    }
}

fn classify_entry(mode: u32, stage: u8, _skip_worktree: bool) -> TrackedPathKind {
    if stage != 0 {
        return TrackedPathKind::Unmerged;
    }
    match mode {
        0o100644 | 0o100755 => TrackedPathKind::Regular,
        0o120000 => TrackedPathKind::Symlink,
        0o160000 => TrackedPathKind::Submodule,
        0o040000 => TrackedPathKind::SparseDirectory,
        _ => TrackedPathKind::UnsupportedMode,
    }
}

fn inspection_reasons(
    checkout_ref: &CheckoutRef,
    split_index: bool,
    tracked: &[TrackedManifestEntry],
    untracked: &[OpaqueRepoPath],
) -> Vec<GuardedUndoReasonCode> {
    let mut reasons = Vec::new();
    if matches!(checkout_ref, CheckoutRef::Detached) {
        push_reason(&mut reasons, GuardedUndoReasonCode::DetachedHead);
    }
    if split_index {
        push_reason(&mut reasons, GuardedUndoReasonCode::SparseOrSkipWorktree);
    }
    if !untracked.is_empty() {
        push_reason(&mut reasons, GuardedUndoReasonCode::UntrackedPath);
    }
    for entry in tracked {
        if entry.assume_unchanged {
            push_reason(&mut reasons, GuardedUndoReasonCode::AssumeUnchanged);
        }
        if entry.skip_worktree || entry.kind == TrackedPathKind::SparseDirectory {
            push_reason(&mut reasons, GuardedUndoReasonCode::SparseOrSkipWorktree);
        }
        match entry.kind {
            TrackedPathKind::Regular | TrackedPathKind::SparseDirectory => {}
            TrackedPathKind::Symlink => {
                push_reason(&mut reasons, GuardedUndoReasonCode::SymlinkOrReparsePoint)
            }
            TrackedPathKind::Submodule => {
                push_reason(&mut reasons, GuardedUndoReasonCode::Submodule)
            }
            TrackedPathKind::Unmerged => {
                push_reason(&mut reasons, GuardedUndoReasonCode::UnmergedPath)
            }
            TrackedPathKind::UnsupportedMode => {
                push_reason(&mut reasons, GuardedUndoReasonCode::UnsupportedStatus)
            }
        }
        for hazard in &entry.attribute_hazards {
            push_reason(&mut reasons, hazard.reason_code());
        }
    }
    reasons
}

fn push_reason(reasons: &mut Vec<GuardedUndoReasonCode>, reason: GuardedUndoReasonCode) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn manifest_digest(entries: &[TrackedManifestEntry]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"DCC_GU_TRACKED_MANIFEST\0\x01");
    hasher.update((entries.len() as u64).to_le_bytes());
    for entry in entries {
        digest_field(&mut hasher, entry.path.as_persisted_bytes());
        hasher.update(entry.mode.to_le_bytes());
        hasher.update([entry.stage]);
        digest_field(&mut hasher, &entry.object_id);
        hasher.update([
            u8::from(entry.assume_unchanged),
            u8::from(entry.skip_worktree),
            kind_code(entry.kind),
        ]);
        hasher.update((entry.attribute_hazards.len() as u64).to_le_bytes());
        for hazard in &entry.attribute_hazards {
            hasher.update([hazard_code(*hazard)]);
        }
    }
    Sha256Digest(hasher.finalize().into())
}

fn kind_code(kind: TrackedPathKind) -> u8 {
    match kind {
        TrackedPathKind::Regular => 1,
        TrackedPathKind::Symlink => 2,
        TrackedPathKind::Submodule => 3,
        TrackedPathKind::SparseDirectory => 4,
        TrackedPathKind::Unmerged => 5,
        TrackedPathKind::UnsupportedMode => 6,
    }
}

fn hazard_code(hazard: AttributeHazard) -> u8 {
    match hazard {
        AttributeHazard::Filter => 1,
        AttributeHazard::WorkingTreeEncoding => 2,
        AttributeHazard::Ident => 3,
        AttributeHazard::Text => 4,
        AttributeHazard::Eol => 5,
        AttributeHazard::Crlf => 6,
    }
}

fn digest_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn nul_records(output: &[u8], step: GitInspectionStep) -> Result<Vec<&[u8]>, GitInspectorError> {
    if output.is_empty() {
        return Ok(Vec::new());
    }
    if !output.ends_with(&[0]) {
        return Err(GitInspectorError::InvalidOutput { step });
    }
    Ok(output[..output.len() - 1]
        .split(|byte| *byte == 0)
        .collect())
}

fn split_once(bytes: &[u8], delimiter: u8) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|byte| *byte == delimiter)?;
    Some((&bytes[..index], &bytes[index + 1..]))
}

fn parse_octal(value: Option<&[u8]>, step: GitInspectionStep) -> Result<u32, GitInspectorError> {
    let value = std::str::from_utf8(value.unwrap_or_default())
        .map_err(|_| GitInspectorError::InvalidOutput { step })?;
    u32::from_str_radix(value, 8).map_err(|_| GitInspectorError::InvalidOutput { step })
}

fn parse_decimal(value: Option<&[u8]>, step: GitInspectionStep) -> Result<u8, GitInspectorError> {
    let value = std::str::from_utf8(value.unwrap_or_default())
        .map_err(|_| GitInspectorError::InvalidOutput { step })?;
    value
        .parse::<u8>()
        .map_err(|_| GitInspectorError::InvalidOutput { step })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::fs;
    #[cfg(target_os = "linux")]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::process::Command;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use tempfile::TempDir;

    #[cfg(target_os = "linux")]
    fn trusted_program(directory: &Path, name: &str, body: &str) -> TrustedGitBinary {
        let source = directory.join(format!("{name}.c"));
        let path = directory.join(name);
        fs::write(
            &source,
            format!(
                "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <unistd.h>\nint main(void) {{ {body} }}\n"
            ),
        )
        .unwrap();
        let compiler = Command::new("which").arg("cc").output().unwrap();
        assert!(compiler.status.success());
        let compiler = String::from_utf8(compiler.stdout).unwrap();
        let status = Command::new(compiler.trim())
            .args(["-o"])
            .arg(&path)
            .arg(&source)
            .status()
            .unwrap();
        assert!(status.success());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let path = fs::canonicalize(path).unwrap();
        TrustedGitBinary::verify_absolute(&path).unwrap()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[derive(Clone, Copy)]
    struct TestIndexReader;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    impl IndexFileReader for TestIndexReader {
        fn observe(
            &self,
            path: &UntrustedGitPath,
            _layout: &UntrustedGitLayout,
            maximum_bytes: u64,
        ) -> Result<IndexObservation, IndexReadError> {
            let path =
                raw_absolute_path(path.as_bytes()).map_err(|_| IndexReadError::Unreadable)?;
            let metadata = fs::symlink_metadata(&path).map_err(|_| IndexReadError::Unreadable)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(IndexReadError::Unsupported);
            }
            if metadata.len() > maximum_bytes {
                return Err(IndexReadError::TooLarge);
            }
            let bytes = fs::read(&path).map_err(|_| IndexReadError::Unreadable)?;
            Ok(IndexObservation {
                sha256: Sha256Digest::of(&bytes),
                size: bytes.len() as u64,
                stat_identity: test_stat_identity(&metadata),
            })
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn test_stat_identity(metadata: &fs::Metadata) -> Vec<u8> {
        use std::os::unix::fs::MetadataExt;
        let mut result = Vec::new();
        for field in [metadata.dev(), metadata.ino(), metadata.size()] {
            result.extend_from_slice(&field.to_le_bytes());
        }
        result
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn git_binary_path() -> PathBuf {
        #[cfg(target_os = "macos")]
        return PathBuf::from("/usr/bin/git");
        #[cfg(target_os = "linux")]
        {
            let output = Command::new("which").arg("git").output().unwrap();
            assert!(output.status.success());
            let raw = String::from_utf8(output.stdout).unwrap();
            fs::canonicalize(raw.trim()).unwrap()
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn trusted_git() -> TrustedGitBinary {
        TrustedGitBinary::verify_absolute(&git_binary_path()).unwrap()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn inspector() -> GitInspector<TestIndexReader> {
        GitInspector::with_index_reader(
            GitInspectorLimits::default(),
            trusted_git(),
            TestIndexReader,
        )
        .unwrap()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git setup command failed: {args:?}");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn repository() -> TempDir {
        let temp = tempfile::tempdir().unwrap();
        git(temp.path(), &["init", "-q"]);
        git(temp.path(), &["config", "user.name", "DCC Test"]);
        git(
            temp.path(),
            &["config", "user.email", "dcc@example.invalid"],
        );
        fs::write(temp.path().join("tracked.txt"), b"tracked\n").unwrap();
        git(temp.path(), &["add", "tracked.txt"]);
        git(temp.path(), &["commit", "-qm", "initial"]);
        temp
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn preserves_non_utf8_paths_and_reports_untracked_without_reading_content() {
        let repo = repository();
        let raw_name = b"raw-\xff.txt".to_vec();
        let name = OsString::from_vec(raw_name.clone());
        fs::write(repo.path().join(&name), b"tracked raw\n").unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .arg("add")
            .arg("--")
            .arg(&name)
            .status()
            .unwrap();
        assert!(status.success());
        git(repo.path(), &["commit", "-qm", "raw path"]);
        fs::write(repo.path().join("untracked.bin"), b"must not be inspected").unwrap();

        let result = inspector().inspect(repo.path()).unwrap();
        assert!(result.tracked.iter().any(|entry| {
            entry.path.as_persisted_bytes().get(2..) == Some(raw_name.as_slice())
        }));
        assert!(result.untracked.iter().any(|path| {
            path.as_persisted_bytes().get(2..) == Some(b"untracked.bin".as_slice())
        }));
    }

    #[test]
    fn tracked_parser_preserves_non_utf8_path_bytes() {
        let mut output = b"H 100644 0123456789012345678901234567890123456789 0\traw-".to_vec();
        output.extend_from_slice(b"\xff.txt\0");
        let entries = parse_tracked_manifest(&output, &GitInspectorLimits::default()).unwrap();
        assert_eq!(&entries[0].path.as_persisted_bytes()[2..], b"raw-\xff.txt");
    }

    #[test]
    fn stdout_drain_reports_limit_plus_one_as_exceeded() {
        let mut reader = std::io::Cursor::new(vec![b'x'; 33]);
        let mut stored = Vec::new();
        assert_eq!(
            drain_nonblocking(&mut reader, &mut stored, 32).unwrap(),
            DrainOutcome::Exceeded
        );
        assert!(stored.len() <= 32);
    }

    #[test]
    fn stderr_drain_reports_limit_plus_one_as_exceeded() {
        let mut reader = std::io::Cursor::new(vec![b'x'; 33]);
        let mut total = 0;
        assert_eq!(
            drain_nonblocking_count(&mut reader, &mut total, 32).unwrap(),
            DrainOutcome::Exceeded
        );
        assert!(total <= 32);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_accepts_only_immutable_system_git_policy() {
        use std::os::unix::fs::symlink;
        let trusted = TrustedGitBinary::verify_absolute(Path::new("/usr/bin/git")).unwrap();
        trusted.revalidate().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let user_git = temp.path().join("git");
        fs::copy("/usr/bin/git", &user_git).unwrap();
        fs::set_permissions(&user_git, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            TrustedGitBinary::verify_absolute(&user_git).unwrap_err(),
            GitInspectorError::UnsupportedPlatform
        );
        let alias = temp.path().join("git-symlink");
        symlink("/usr/bin/git", &alias).unwrap();
        assert_eq!(
            TrustedGitBinary::verify_absolute(&alias).unwrap_err(),
            GitInspectorError::UnsupportedPlatform
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_policy_rejects_hostile_owner_modes_and_identity_change() {
        assert!(macos_system_component_is_safe(0, 0o755, true, false, false));
        assert!(macos_system_component_is_safe(0, 0o755, false, true, true));
        assert!(!macos_system_component_is_safe(
            501, 0o755, true, false, false
        ));
        assert!(!macos_system_component_is_safe(
            0, 0o777, true, false, false
        ));
        assert!(!macos_system_component_is_safe(0, 0o644, false, true, true));
        assert!(macos_identity_matches(b"physical-a", b"physical-a"));
        assert!(!macos_identity_matches(b"physical-a", b"physical-b"));
    }

    #[test]
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn platforms_without_secure_exec_by_fd_fail_closed() {
        let error = TrustedGitBinary::verify_absolute(Path::new("/usr/bin/git")).unwrap_err();
        assert_eq!(error, GitInspectorError::UnsupportedPlatform);
        assert_eq!(
            error.reason_code(),
            GuardedUndoReasonCode::AdapterUnsupported
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn configured_helpers_filters_drivers_editors_and_pagers_are_never_invoked() {
        let repo = repository();
        let sentinel = repo.path().join("SENTINEL");
        let helper = repo.path().join("hostile-helper.sh");
        fs::write(
            &helper,
            format!(
                "#!/bin/sh\nprintf invoked > '{}'\nexit 99\n",
                sentinel.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            repo.path().join(".gitattributes"),
            b"*.txt filter=hostile diff=hostile ident text eol=crlf\n",
        )
        .unwrap();
        let helper_value = helper.to_string_lossy();
        git(
            repo.path(),
            &["config", "filter.hostile.clean", &helper_value],
        );
        git(
            repo.path(),
            &["config", "filter.hostile.process", &helper_value],
        );
        git(
            repo.path(),
            &["config", "diff.hostile.command", &helper_value],
        );
        git(repo.path(), &["config", "diff.external", &helper_value]);
        git(repo.path(), &["config", "core.fsmonitor", &helper_value]);
        git(repo.path(), &["config", "credential.helper", &helper_value]);
        git(repo.path(), &["config", "core.pager", &helper_value]);
        git(repo.path(), &["config", "core.editor", &helper_value]);

        let result = inspector().inspect(repo.path()).unwrap();
        let tracked = result
            .tracked
            .iter()
            .find(|entry| entry.path.as_persisted_bytes().ends_with(b"tracked.txt"))
            .unwrap();
        assert!(tracked.attribute_hazards.contains(&AttributeHazard::Filter));
        assert!(tracked.attribute_hazards.contains(&AttributeHazard::Ident));
        assert!(tracked.attribute_hazards.contains(&AttributeHazard::Text));
        assert!(tracked.attribute_hazards.contains(&AttributeHazard::Eol));
        assert!(!sentinel.exists());
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn detached_head_is_classified_without_losing_the_raw_oid() {
        let repo = repository();
        git(repo.path(), &["checkout", "--detach", "-q"]);
        let result = inspector().inspect(repo.path()).unwrap();
        assert_eq!(result.checkout_ref, CheckoutRef::Detached);
        assert!(matches!(result.head_oid.len(), 20 | 32));
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn submodule_sparse_assume_unchanged_and_split_index_are_classified() {
        let repo = repository();
        let head = Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let head = String::from_utf8(head.stdout).unwrap();
        let cacheinfo = format!("160000,{},nested", head.trim());
        git(
            repo.path(),
            &["update-index", "--add", "--cacheinfo", &cacheinfo],
        );
        git(
            repo.path(),
            &["update-index", "--skip-worktree", "tracked.txt"],
        );
        git(
            repo.path(),
            &["update-index", "--assume-unchanged", "tracked.txt"],
        );
        git(repo.path(), &["update-index", "--split-index"]);

        let result = inspector().inspect(repo.path()).unwrap();
        assert!(result.split_index);
        assert!(result
            .tracked
            .iter()
            .any(|entry| entry.kind == TrackedPathKind::Submodule));
        let tracked = result
            .tracked
            .iter()
            .find(|entry| entry.path.as_persisted_bytes().ends_with(b"tracked.txt"))
            .unwrap();
        assert!(tracked.skip_worktree);
        assert!(tracked.assume_unchanged);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn bare_repository_fails_closed_with_the_stable_reason() {
        let temp = tempfile::tempdir().unwrap();
        git(temp.path(), &["init", "--bare", "-q"]);
        let error = inspector().inspect(temp.path()).unwrap_err();
        assert_eq!(error, GitInspectorError::BareRepository);
        assert_eq!(error.reason_code(), GuardedUndoReasonCode::BareRepository);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn trusted_runner_does_not_inherit_path_home_or_git_config_injection() {
        if std::env::var_os("DCC_GIT_INSPECTOR_HOSTILE_CHILD").is_none() {
            let temp = tempfile::tempdir().unwrap();
            let hostile_global = temp.path().join("hostile-global.gitconfig");
            let hostile_system = temp.path().join("hostile-system.gitconfig");
            fs::write(&hostile_global, b"[core]\n\tfsmonitor = hostile\n").unwrap();
            fs::write(&hostile_system, b"[credential]\n\thelper = hostile\n").unwrap();
            let inherited_path = std::env::var_os("PATH").unwrap_or_default();
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "git_inspector::tests::trusted_runner_does_not_inherit_path_home_or_git_config_injection",
                ])
                .env("DCC_GIT_INSPECTOR_HOSTILE_CHILD", "1")
                .env(
                    "PATH",
                    format!("DCC_HOSTILE_PATH:{}", inherited_path.to_string_lossy()),
                )
                .env("HOME", temp.path())
                .env("XDG_CONFIG_HOME", temp.path())
                .env("GIT_CONFIG_GLOBAL", &hostile_global)
                .env("GIT_CONFIG_SYSTEM", &hostile_system)
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let sentinel = temp.path().join("ENV_SENTINEL");
        let program = trusted_program(
            temp.path(),
            "git-env-sentinel",
            &format!(
                "int unsafe = 0; const char *v = getenv(\"PATH\"); if (v && strstr(v, \"DCC_HOSTILE_PATH\")) unsafe = 1; if (!(v = getenv(\"HOME\")) || strcmp(v, \"/dev/null\")) unsafe = 1; if (!(v = getenv(\"XDG_CONFIG_HOME\")) || strcmp(v, \"/dev/null\")) unsafe = 1; if (!(v = getenv(\"GIT_CONFIG_NOSYSTEM\")) || strcmp(v, \"1\")) unsafe = 1; if (!(v = getenv(\"GIT_CONFIG_GLOBAL\")) || strcmp(v, \"/dev/null\")) unsafe = 1; if (!(v = getenv(\"GIT_CONFIG_SYSTEM\")) || strcmp(v, \"/dev/null\")) unsafe = 1; if (unsafe) {{ FILE *f = fopen(\"{}\", \"w\"); if (f) fclose(f); }} puts(\"false\"); return 0;",
                sentinel.display().to_string().replace('\\', "\\\\").replace('"', "\\\"")
            ),
        );
        let output = run_bounded_git(
            &program,
            temp.path(),
            GitBuiltin::RevParse,
            &["--is-bare-repository"],
            &[],
            Duration::from_secs(1),
            128,
            GitInspectionStep::BareRepository,
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"false\n");
        assert!(!sentinel.exists());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn descendant_holding_a_pipe_cannot_make_timeout_join_forever() {
        let temp = tempfile::tempdir().unwrap();
        let program = trusted_program(
            temp.path(),
            "git-pipe-holder",
            "if (fork() == 0) { sleep(30); return 0; } puts(\"false\"); return 0;",
        );
        let started = Instant::now();
        let error = match run_bounded_git(
            &program,
            temp.path(),
            GitBuiltin::RevParse,
            &["--is-bare-repository"],
            &[],
            Duration::from_millis(100),
            128,
            GitInspectionStep::BareRepository,
        ) {
            Err(error) => error,
            Ok(_) => panic!("pipe-holding descendant must reach the bounded timeout"),
        };
        assert_eq!(error, GitInspectorError::Timeout);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn platform_reader_rejection_blocks_untrusted_index_path_without_direct_read() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        struct RejectingPlatformReader(Arc<AtomicUsize>);
        impl IndexFileReader for RejectingPlatformReader {
            fn observe(
                &self,
                path: &UntrustedGitPath,
                _layout: &UntrustedGitLayout,
                _maximum_bytes: u64,
            ) -> Result<IndexObservation, IndexReadError> {
                assert!(path.as_bytes().ends_with(b"index"));
                self.0.fetch_add(1, Ordering::SeqCst);
                Err(IndexReadError::Unsupported)
            }
        }

        let repo = repository();
        let calls = Arc::new(AtomicUsize::new(0));
        let inspector = GitInspector::with_index_reader(
            GitInspectorLimits::default(),
            trusted_git(),
            RejectingPlatformReader(Arc::clone(&calls)),
        )
        .unwrap();
        let error = inspector.inspect(repo.path()).unwrap_err();
        assert_eq!(error, GitInspectorError::UnsupportedPlatform);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn trusted_git_binary_rejects_relative_and_symlink_paths() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let _trusted = trusted_program(temp.path(), "real-git", "return 0;");
        let real_path = fs::canonicalize(temp.path().join("real-git")).unwrap();
        let alias = temp.path().join("git-alias");
        symlink(&real_path, &alias).unwrap();
        assert_eq!(
            TrustedGitBinary::verify_absolute(Path::new("relative-git")).unwrap_err(),
            GitInspectorError::UntrustedGitBinary
        );
        assert_eq!(
            TrustedGitBinary::verify_absolute(&alias).unwrap_err(),
            GitInspectorError::UntrustedGitBinary
        );
        let writable = temp.path().join("writable-git");
        fs::copy(&real_path, &writable).unwrap();
        fs::set_permissions(&writable, fs::Permissions::from_mode(0o722)).unwrap();
        assert_eq!(
            TrustedGitBinary::verify_absolute(&writable).unwrap_err(),
            GitInspectorError::UntrustedGitBinary
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn trusted_git_executes_open_object_after_path_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let selected_path = temp.path().join("selected-git");
        fs::copy(git_binary_path(), &selected_path).unwrap();
        fs::set_permissions(&selected_path, fs::Permissions::from_mode(0o700)).unwrap();
        let selected_path = fs::canonicalize(&selected_path).unwrap();
        let trusted = TrustedGitBinary::verify_absolute(&selected_path).unwrap();
        fs::rename(&selected_path, temp.path().join("original-held")).unwrap();
        fs::copy("/usr/bin/false", &selected_path).unwrap();
        fs::set_permissions(&selected_path, fs::Permissions::from_mode(0o700)).unwrap();

        let output = run_bounded_git(
            &trusted,
            temp.path(),
            GitBuiltin::RevParse,
            &["--is-bare-repository"],
            &[],
            Duration::from_secs(1),
            128,
            GitInspectionStep::BareRepository,
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"false\n");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn endless_stdout_and_limit_plus_one_stderr_kill_the_process_group() {
        let yes_path = fs::canonicalize("/usr/bin/yes").unwrap();
        let yes = TrustedGitBinary::verify_absolute(&yes_path).unwrap();
        let stdout_error = match run_bounded_git(
            &yes,
            Path::new("/"),
            GitBuiltin::LsFiles,
            &[],
            &[],
            Duration::from_secs(1),
            32,
            GitInspectionStep::TrackedManifest,
        ) {
            Err(error) => error,
            Ok(_) => panic!("endless stdout must exceed the bound"),
        };
        assert_eq!(
            stdout_error,
            GitInspectorError::OutputLimit {
                step: GitInspectionStep::TrackedManifest
            }
        );

        let huge = "x".repeat(MAX_STDERR_BYTES + 1);
        let stderr_error = match run_bounded_git(
            &trusted_git(),
            Path::new("/"),
            GitBuiltin::RevParse,
            &[&huge],
            &[],
            Duration::from_secs(1),
            128,
            GitInspectionStep::Head,
        ) {
            Err(error) => error,
            Ok(_) => panic!("limit+1 stderr must exceed the bound"),
        };
        assert_eq!(
            stderr_error,
            GitInspectorError::OutputLimit {
                step: GitInspectionStep::Head
            }
        );
    }
}
