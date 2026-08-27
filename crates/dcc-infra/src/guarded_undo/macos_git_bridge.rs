//! Physical macOS validation for logical paths emitted by Git.
//!
//! This bridge deliberately accepts only layouts that can be proven to live
//! below the retained workspace descriptor. Logical paths are never used for
//! filesystem access before conversion to a validated repository-relative
//! byte path.

#![cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]

use std::{
    fmt,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use dcc_core::domain::guarded_undo::{OpaqueRepoPath, PhysicalRootId, RegularFileMetadataV1};

use super::{
    git_inspector::{
        IndexFileReader, IndexObservation, IndexReadError, UntrustedGitLayout, UntrustedGitPath,
    },
    macos_root::{MacWorkspaceRoot, MacWorkspaceRootError, StableDigestObservation},
};

const MAX_GITDIR_FILE_BYTES: u64 = 4 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MacGitBridgeError {
    InvalidWorkspace,
    LayoutMismatch,
    LayoutEscape,
    UnsafeGitMetadata,
    UnsupportedLayout,
    IndexUnreadable,
    IndexTooLarge,
    IndexChanged,
}

impl fmt::Display for MacGitBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidWorkspace => "workspace identity is invalid",
            Self::LayoutMismatch => "Git layout does not match the retained workspace",
            Self::LayoutEscape => "Git layout escapes the retained workspace",
            Self::UnsafeGitMetadata => "Git metadata has unsupported physical properties",
            Self::UnsupportedLayout => "Git metadata layout is unsupported",
            Self::IndexUnreadable => "active index is unreadable",
            Self::IndexTooLarge => "active index exceeds its byte bound",
            Self::IndexChanged => "active index changed after physical validation",
        })
    }
}

impl std::error::Error for MacGitBridgeError {}

/// A physically bound active-index reader for one inspected Git layout.
pub(crate) struct MacGitBridge {
    root: Arc<MacWorkspaceRoot>,
    layout: UntrustedGitLayout,
    active_index_logical: Vec<u8>,
    active_index_relative: OpaqueRepoPath,
    baseline: StableDigestObservation,
    git_dir_id: PhysicalRootId,
    common_dir_id: PhysicalRootId,
}

impl fmt::Debug for MacGitBridge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacGitBridge")
            .field("paths", &"[redacted]")
            .field("index_size", &self.baseline.size)
            .finish_non_exhaustive()
    }
}

impl MacGitBridge {
    pub(crate) fn bind(
        root: Arc<MacWorkspaceRoot>,
        workspace_absolute: &[u8],
        layout: &UntrustedGitLayout,
        maximum_index_bytes: u64,
    ) -> Result<Self, MacGitBridgeError> {
        validate_absolute(workspace_absolute).map_err(|_| MacGitBridgeError::InvalidWorkspace)?;
        if layout.worktree.as_bytes() != workspace_absolute {
            return Err(MacGitBridgeError::LayoutMismatch);
        }

        let workspace_path =
            PathBuf::from(std::ffi::OsString::from_vec(workspace_absolute.to_vec()));
        let reopened = MacWorkspaceRoot::open_absolute(Path::new(&workspace_path))
            .map_err(|_| MacGitBridgeError::InvalidWorkspace)?;
        if reopened.physical_root_id() != root.physical_root_id() {
            return Err(MacGitBridgeError::LayoutMismatch);
        }

        let git_dir_relative = internal_path(workspace_absolute, layout.git_dir.as_bytes())?;
        let common_dir_relative = internal_path(workspace_absolute, layout.common_dir.as_bytes())?;
        let active_index_relative =
            internal_path(workspace_absolute, layout.active_index_path.as_bytes())?;

        let git_dir_id = validate_git_dir(&root, workspace_absolute, &git_dir_relative)?;
        let common_dir_id = root
            .validate_relative_directory(&common_dir_relative)
            .map_err(|_| MacGitBridgeError::UnsafeGitMetadata)?;

        let expected_index = append_path(layout.git_dir.as_bytes(), b"index")
            .ok_or(MacGitBridgeError::UnsupportedLayout)?;
        if layout.active_index_path.as_bytes() != expected_index.as_slice() {
            return Err(MacGitBridgeError::LayoutMismatch);
        }

        let baseline = root
            .observe_index_stable(&active_index_relative, maximum_index_bytes)
            .map_err(map_index_error)?;

        Ok(Self {
            root,
            layout: layout.clone(),
            active_index_logical: layout.active_index_path.as_bytes().to_vec(),
            active_index_relative,
            baseline,
            git_dir_id,
            common_dir_id,
        })
    }

    #[cfg(test)]
    fn directory_ids(&self) -> (&PhysicalRootId, &PhysicalRootId) {
        (&self.git_dir_id, &self.common_dir_id)
    }
}

impl IndexFileReader for MacGitBridge {
    fn observe(
        &self,
        path: &UntrustedGitPath,
        layout: &UntrustedGitLayout,
        maximum_bytes: u64,
    ) -> Result<IndexObservation, IndexReadError> {
        if layout != &self.layout || path.as_bytes() != self.active_index_logical {
            return Err(IndexReadError::Unsupported);
        }
        let current = self
            .root
            .observe_index_stable(&self.active_index_relative, maximum_bytes)
            .map_err(map_reader_error)?;
        if current != self.baseline {
            return Err(IndexReadError::Changed);
        }
        Ok(IndexObservation {
            sha256: current.sha256,
            size: current.size,
            stat_identity: current.stat_identity,
        })
    }
}

/// Reader passed to [`GitInspector`](super::git_inspector::GitInspector).
///
/// It owns only the retained workspace handle and its canonical raw absolute
/// name. Every observation binds and revalidates the complete logical layout,
/// so the inspector's second observation cannot reuse stale physical proof.
pub(crate) struct MacIndexFileReader {
    root: Arc<MacWorkspaceRoot>,
    workspace_absolute: Vec<u8>,
}

impl fmt::Debug for MacIndexFileReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacIndexFileReader")
            .field("workspace", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl MacIndexFileReader {
    pub(crate) fn new(
        root: Arc<MacWorkspaceRoot>,
        workspace_absolute: Vec<u8>,
    ) -> Result<Self, MacGitBridgeError> {
        validate_absolute(&workspace_absolute).map_err(|_| MacGitBridgeError::InvalidWorkspace)?;
        let workspace_path =
            PathBuf::from(std::ffi::OsString::from_vec(workspace_absolute.clone()));
        let reopened = MacWorkspaceRoot::open_absolute(Path::new(&workspace_path))
            .map_err(|_| MacGitBridgeError::InvalidWorkspace)?;
        if reopened.physical_root_id() != root.physical_root_id() {
            return Err(MacGitBridgeError::LayoutMismatch);
        }
        Ok(Self {
            root,
            workspace_absolute,
        })
    }
}

impl IndexFileReader for MacIndexFileReader {
    fn observe(
        &self,
        path: &UntrustedGitPath,
        layout: &UntrustedGitLayout,
        maximum_bytes: u64,
    ) -> Result<IndexObservation, IndexReadError> {
        let bridge = MacGitBridge::bind(
            Arc::clone(&self.root),
            &self.workspace_absolute,
            layout,
            maximum_bytes,
        )
        .map_err(map_bridge_reader_error)?;
        bridge.observe(path, layout, maximum_bytes)
    }
}

fn validate_git_dir(
    root: &MacWorkspaceRoot,
    workspace_absolute: &[u8],
    layout_git_dir_relative: &OpaqueRepoPath,
) -> Result<PhysicalRootId, MacGitBridgeError> {
    let dot_git = opaque(b".git")?;
    if let Ok(dot_git_id) = root.validate_relative_directory(&dot_git) {
        let layout_id = root
            .validate_relative_directory(layout_git_dir_relative)
            .map_err(|_| MacGitBridgeError::UnsafeGitMetadata)?;
        if layout_id != dot_git_id {
            return Err(MacGitBridgeError::LayoutMismatch);
        }
        return Ok(layout_id);
    }

    let capture = root
        .read_stable_twice(&dot_git, MAX_GITDIR_FILE_BYTES, None)
        .map_err(|_| MacGitBridgeError::UnsafeGitMetadata)?;
    validate_gitdir_file_metadata(&capture.metadata)?;
    let target = parse_gitdir_file(capture.bytes.as_slice())?;
    let target_relative = if target.first() == Some(&b'/') {
        internal_path(workspace_absolute, target)?
    } else {
        opaque(target)?
    };
    let target_id = root
        .validate_relative_directory(&target_relative)
        .map_err(|_| MacGitBridgeError::UnsafeGitMetadata)?;
    let layout_id = root
        .validate_relative_directory(layout_git_dir_relative)
        .map_err(|_| MacGitBridgeError::UnsafeGitMetadata)?;
    if target_id != layout_id {
        return Err(MacGitBridgeError::LayoutMismatch);
    }
    Ok(layout_id)
}

fn validate_gitdir_file_metadata(
    metadata: &RegularFileMetadataV1,
) -> Result<(), MacGitBridgeError> {
    if metadata.adapter != "macos" || metadata.link_count != 1 {
        return Err(MacGitBridgeError::UnsafeGitMetadata);
    }
    let mode = metadata
        .fields
        .get("mode")
        .and_then(|value| value.as_slice().try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(MacGitBridgeError::UnsafeGitMetadata)?;
    let uid = metadata
        .fields
        .get("uid")
        .and_then(|value| value.as_slice().try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(MacGitBridgeError::UnsafeGitMetadata)?;
    let forbidden = (libc::S_ISUID | libc::S_ISGID | libc::S_IWGRP | libc::S_IWOTH) as u32;
    if uid != unsafe { libc::geteuid() } || mode & forbidden != 0 {
        return Err(MacGitBridgeError::UnsafeGitMetadata);
    }
    Ok(())
}

fn parse_gitdir_file(bytes: &[u8]) -> Result<&[u8], MacGitBridgeError> {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let target = bytes
        .strip_prefix(b"gitdir: ")
        .ok_or(MacGitBridgeError::UnsupportedLayout)?;
    if target.is_empty() || target.iter().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
        return Err(MacGitBridgeError::UnsupportedLayout);
    }
    if target.first() == Some(&b'/') {
        validate_absolute(target).map_err(|_| MacGitBridgeError::LayoutEscape)?;
    } else {
        validate_relative(target)?;
    }
    Ok(target)
}

fn internal_path(
    workspace_absolute: &[u8],
    candidate_absolute: &[u8],
) -> Result<OpaqueRepoPath, MacGitBridgeError> {
    validate_absolute(candidate_absolute).map_err(|_| MacGitBridgeError::LayoutEscape)?;
    let suffix = candidate_absolute
        .strip_prefix(workspace_absolute)
        .and_then(|suffix| suffix.strip_prefix(b"/"))
        .ok_or(MacGitBridgeError::LayoutEscape)?;
    opaque(suffix)
}

fn opaque(relative: &[u8]) -> Result<OpaqueRepoPath, MacGitBridgeError> {
    validate_relative(relative)?;
    OpaqueRepoPath::unix(relative).map_err(|_| MacGitBridgeError::LayoutEscape)
}

fn validate_absolute(path: &[u8]) -> Result<(), ()> {
    if path.len() < 2 || path.first() != Some(&b'/') || path.last() == Some(&b'/') {
        return Err(());
    }
    validate_components(&path[1..])
}

fn validate_relative(path: &[u8]) -> Result<(), MacGitBridgeError> {
    validate_components(path).map_err(|_| MacGitBridgeError::LayoutEscape)
}

fn validate_components(path: &[u8]) -> Result<(), ()> {
    if path.is_empty()
        || path.iter().any(|byte| *byte == 0)
        || path
            .split(|byte| *byte == b'/')
            .any(|component| component.is_empty() || component == b"." || component == b"..")
    {
        return Err(());
    }
    Ok(())
}

fn append_path(directory: &[u8], basename: &[u8]) -> Option<Vec<u8>> {
    if directory.is_empty() || directory.last() == Some(&b'/') || basename.contains(&b'/') {
        return None;
    }
    let mut path = Vec::with_capacity(directory.len().checked_add(1 + basename.len())?);
    path.extend_from_slice(directory);
    path.push(b'/');
    path.extend_from_slice(basename);
    Some(path)
}

fn map_index_error(error: MacWorkspaceRootError) -> MacGitBridgeError {
    match error {
        MacWorkspaceRootError::FileTooLarge => MacGitBridgeError::IndexTooLarge,
        MacWorkspaceRootError::FileChanged => MacGitBridgeError::IndexChanged,
        MacWorkspaceRootError::InvalidPath
        | MacWorkspaceRootError::Io(_)
        | MacWorkspaceRootError::AdapterUnsupported => MacGitBridgeError::IndexUnreadable,
    }
}

fn map_reader_error(error: MacWorkspaceRootError) -> IndexReadError {
    match error {
        MacWorkspaceRootError::FileTooLarge => IndexReadError::TooLarge,
        MacWorkspaceRootError::FileChanged => IndexReadError::Changed,
        MacWorkspaceRootError::InvalidPath | MacWorkspaceRootError::AdapterUnsupported => {
            IndexReadError::Unsupported
        }
        MacWorkspaceRootError::Io(_) => IndexReadError::Unreadable,
    }
}

fn map_bridge_reader_error(error: MacGitBridgeError) -> IndexReadError {
    match error {
        MacGitBridgeError::IndexTooLarge => IndexReadError::TooLarge,
        MacGitBridgeError::IndexChanged => IndexReadError::Changed,
        MacGitBridgeError::IndexUnreadable => IndexReadError::Unreadable,
        MacGitBridgeError::InvalidWorkspace
        | MacGitBridgeError::LayoutMismatch
        | MacGitBridgeError::LayoutEscape
        | MacGitBridgeError::UnsafeGitMetadata
        | MacGitBridgeError::UnsupportedLayout => IndexReadError::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::guarded_undo::MAX_INDEX_BYTES;
    use std::{
        ffi::CString,
        fs,
        os::unix::{
            ffi::OsStrExt,
            fs::{symlink, PermissionsExt},
        },
        process::Command,
        sync::atomic::{AtomicBool, Ordering},
    };

    use crate::guarded_undo::git_inspector::{
        GitInspector, GitInspectorError, GitInspectorLimits, TrustedGitBinary,
    };

    struct Fixture {
        directory: tempfile::TempDir,
        workspace: Vec<u8>,
        root: Arc<MacWorkspaceRoot>,
    }

    impl Fixture {
        fn empty() -> Self {
            let directory = tempfile::tempdir_in("/private/tmp").unwrap();
            remove_xattrs(directory.path());
            let canonical = fs::canonicalize(directory.path()).unwrap();
            let workspace = canonical.as_os_str().as_bytes().to_vec();
            let root = Arc::new(MacWorkspaceRoot::open_absolute(&canonical).unwrap());
            Self {
                directory,
                workspace,
                root,
            }
        }

        fn dot_git_directory() -> Self {
            let fixture = Self::empty();
            fs::create_dir(fixture.directory.path().join(".git")).unwrap();
            fs::write(fixture.directory.path().join(".git/index"), b"DIRC-index").unwrap();
            remove_xattrs(&fixture.directory.path().join(".git"));
            remove_xattrs(&fixture.directory.path().join(".git/index"));
            fixture
        }

        fn real_git_repository() -> Self {
            let directory = tempfile::tempdir_in("/private/tmp").unwrap();
            run_git(directory.path(), &["init", "-q"]);
            fs::write(directory.path().join("tracked.txt"), b"tracked\n").unwrap();
            run_git(directory.path(), &["add", "--", "tracked.txt"]);
            run_git(
                directory.path(),
                &[
                    "-c",
                    "user.name=DCC Test",
                    "-c",
                    "user.email=dcc@example.invalid",
                    "commit",
                    "--no-gpg-sign",
                    "--no-verify",
                    "-qm",
                    "initial",
                ],
            );
            for path in [
                directory.path().to_path_buf(),
                directory.path().join(".git"),
                directory.path().join(".git/index"),
            ] {
                remove_xattrs(&path);
            }
            let canonical = fs::canonicalize(directory.path()).unwrap();
            let workspace = canonical.as_os_str().as_bytes().to_vec();
            let root = Arc::new(MacWorkspaceRoot::open_absolute(&canonical).unwrap());
            Self {
                directory,
                workspace,
                root,
            }
        }

        fn absolute(&self, relative: &[u8]) -> Vec<u8> {
            let mut result = self.workspace.clone();
            result.push(b'/');
            result.extend_from_slice(relative);
            result
        }

        fn layout(&self, git_dir: &[u8], common_dir: &[u8], index: &[u8]) -> UntrustedGitLayout {
            UntrustedGitLayout {
                git_dir: path(self.absolute(git_dir)),
                common_dir: path(self.absolute(common_dir)),
                worktree: path(self.workspace.clone()),
                active_index_path: path(self.absolute(index)),
            }
        }
    }

    fn path(bytes: Vec<u8>) -> UntrustedGitPath {
        UntrustedGitPath::from_test_bytes(bytes)
    }

    fn run_git(workspace: &Path, arguments: &[&str]) {
        let status = Command::new("/usr/bin/git")
            .current_dir(workspace)
            .args(arguments)
            .status()
            .unwrap();
        assert!(status.success(), "system Git fixture command failed");
    }

    fn remove_xattrs(path: &Path) {
        let path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let size = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0, 0) };
        if size <= 0 {
            return;
        }
        let mut names = vec![0_u8; size as usize];
        let actual = unsafe {
            libc::listxattr(
                path.as_ptr(),
                names.as_mut_ptr() as *mut libc::c_char,
                names.len(),
                0,
            )
        };
        if actual <= 0 {
            return;
        }
        for name in names[..actual as usize]
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = CString::new(name).unwrap();
            unsafe { libc::removexattr(path.as_ptr(), name.as_ptr(), 0) };
        }
    }

    #[test]
    fn binds_dot_git_directory_and_reads_only_bound_index() {
        let fixture = Fixture::dot_git_directory();
        let layout = fixture.layout(b".git", b".git", b".git/index");
        let bridge = MacGitBridge::bind(
            fixture.root.clone(),
            &fixture.workspace,
            &layout,
            MAX_INDEX_BYTES,
        )
        .unwrap();
        let observation = bridge
            .observe(&layout.active_index_path, &layout, MAX_INDEX_BYTES)
            .unwrap();
        assert_eq!(observation.size, 10);
        assert_eq!(bridge.directory_ids().0, bridge.directory_ids().1);

        let other = path(fixture.absolute(b".git/other"));
        assert_eq!(
            bridge.observe(&other, &layout, MAX_INDEX_BYTES),
            Err(IndexReadError::Unsupported)
        );
    }

    #[test]
    fn binds_internal_gitdir_file_indirection() {
        let fixture = Fixture::empty();
        fs::create_dir(fixture.directory.path().join("meta")).unwrap();
        fs::create_dir(fixture.directory.path().join("meta/gitdir")).unwrap();
        fs::write(
            fixture.directory.path().join("meta/gitdir/index"),
            b"DIRC-linked",
        )
        .unwrap();
        fs::write(
            fixture.directory.path().join(".git"),
            b"gitdir: meta/gitdir\n",
        )
        .unwrap();
        fs::set_permissions(
            fixture.directory.path().join(".git"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        for relative in ["meta", "meta/gitdir", "meta/gitdir/index", ".git"] {
            remove_xattrs(&fixture.directory.path().join(relative));
        }

        let layout = fixture.layout(b"meta/gitdir", b"meta/gitdir", b"meta/gitdir/index");
        let bridge = MacGitBridge::bind(
            fixture.root.clone(),
            &fixture.workspace,
            &layout,
            MAX_INDEX_BYTES,
        )
        .unwrap();
        assert_eq!(
            bridge
                .observe(&layout.active_index_path, &layout, MAX_INDEX_BYTES)
                .unwrap()
                .size,
            11
        );
    }

    #[test]
    fn binds_internal_absolute_gitdir_file_indirection() {
        let fixture = Fixture::empty();
        fs::create_dir(fixture.directory.path().join("meta")).unwrap();
        fs::create_dir(fixture.directory.path().join("meta/gitdir")).unwrap();
        fs::write(
            fixture.directory.path().join("meta/gitdir/index"),
            b"DIRC-linked",
        )
        .unwrap();
        let mut contents = b"gitdir: ".to_vec();
        contents.extend_from_slice(&fixture.absolute(b"meta/gitdir"));
        contents.push(b'\n');
        fs::write(fixture.directory.path().join(".git"), contents).unwrap();
        fs::set_permissions(
            fixture.directory.path().join(".git"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        for relative in ["meta", "meta/gitdir", "meta/gitdir/index", ".git"] {
            remove_xattrs(&fixture.directory.path().join(relative));
        }

        let layout = fixture.layout(b"meta/gitdir", b"meta/gitdir", b"meta/gitdir/index");
        MacGitBridge::bind(
            fixture.root.clone(),
            &fixture.workspace,
            &layout,
            MAX_INDEX_BYTES,
        )
        .unwrap();
    }

    #[test]
    fn rejects_external_absolute_escape_and_prefix_spoof() {
        let fixture = Fixture::dot_git_directory();
        let mut external = fixture.layout(b".git", b".git", b".git/index");
        external.git_dir = path(b"/private/tmp/outside/.git".to_vec());
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &external,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::LayoutEscape
        );

        let mut spoof = fixture.workspace.clone();
        spoof.extend_from_slice(b"-evil/.git");
        let mut layout = fixture.layout(b".git", b".git", b".git/index");
        layout.git_dir = path(spoof);
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &layout,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::LayoutEscape
        );
    }

    #[test]
    fn rejects_gitdir_parent_escape_and_symlink() {
        let fixture = Fixture::empty();
        fs::write(
            fixture.directory.path().join(".git"),
            b"gitdir: ../outside\n",
        )
        .unwrap();
        fs::set_permissions(
            fixture.directory.path().join(".git"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        remove_xattrs(&fixture.directory.path().join(".git"));
        let layout = fixture.layout(b"metadata", b"metadata", b"metadata/index");
        assert!(MacGitBridge::bind(
            fixture.root.clone(),
            &fixture.workspace,
            &layout,
            MAX_INDEX_BYTES
        )
        .is_err());

        fs::remove_file(fixture.directory.path().join(".git")).unwrap();
        symlink("/private/tmp", fixture.directory.path().join(".git")).unwrap();
        let layout = fixture.layout(b".git", b".git", b".git/index");
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &layout,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::UnsafeGitMetadata
        );
    }

    #[test]
    fn rejects_group_writable_gitdir_file() {
        let fixture = Fixture::empty();
        fs::create_dir(fixture.directory.path().join("metadata")).unwrap();
        fs::write(
            fixture.directory.path().join("metadata/index"),
            b"DIRC-index",
        )
        .unwrap();
        fs::write(fixture.directory.path().join(".git"), b"gitdir: metadata\n").unwrap();
        fs::set_permissions(
            fixture.directory.path().join(".git"),
            fs::Permissions::from_mode(0o620),
        )
        .unwrap();
        for relative in ["metadata", "metadata/index", ".git"] {
            remove_xattrs(&fixture.directory.path().join(relative));
        }
        let layout = fixture.layout(b"metadata", b"metadata", b"metadata/index");
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &layout,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::UnsafeGitMetadata
        );
    }

    #[test]
    fn rejects_worktree_and_index_layout_mismatch() {
        let fixture = Fixture::dot_git_directory();
        let mut wrong_worktree = fixture.layout(b".git", b".git", b".git/index");
        wrong_worktree.worktree = path(fixture.absolute(b"nested"));
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &wrong_worktree,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::LayoutMismatch
        );

        fs::write(
            fixture.directory.path().join(".git/not-index"),
            b"DIRC-other",
        )
        .unwrap();
        remove_xattrs(&fixture.directory.path().join(".git/not-index"));
        let wrong_index = fixture.layout(b".git", b".git", b".git/not-index");
        assert_eq!(
            MacGitBridge::bind(
                fixture.root.clone(),
                &fixture.workspace,
                &wrong_index,
                MAX_INDEX_BYTES
            )
            .unwrap_err(),
            MacGitBridgeError::LayoutMismatch
        );
    }

    #[test]
    fn detects_index_replacement_after_binding() {
        let fixture = Fixture::dot_git_directory();
        let layout = fixture.layout(b".git", b".git", b".git/index");
        let bridge = MacGitBridge::bind(
            fixture.root.clone(),
            &fixture.workspace,
            &layout,
            MAX_INDEX_BYTES,
        )
        .unwrap();
        let index = fixture.directory.path().join(".git/index");
        fs::write(&index, b"DIRC-replaced").unwrap();
        remove_xattrs(&index);
        assert_eq!(
            bridge.observe(&layout.active_index_path, &layout, MAX_INDEX_BYTES),
            Err(IndexReadError::Changed)
        );
    }

    #[test]
    fn system_git_inspection_uses_physically_bound_reader_end_to_end() {
        let fixture = Fixture::real_git_repository();
        let reader =
            MacIndexFileReader::new(fixture.root.clone(), fixture.workspace.clone()).unwrap();
        let git = TrustedGitBinary::verify_absolute(Path::new("/usr/bin/git")).unwrap();
        let inspector =
            GitInspector::with_index_reader(GitInspectorLimits::default(), git, reader).unwrap();
        let inspection = inspector.inspect(fixture.directory.path()).unwrap();
        assert_eq!(inspection.tracked.len(), 1);
        assert_eq!(inspection.layout.worktree.as_bytes(), fixture.workspace);

        let reader =
            MacIndexFileReader::new(fixture.root.clone(), fixture.workspace.clone()).unwrap();
        let mut mismatched = inspection.layout.clone();
        mismatched.worktree = path(fixture.absolute(b"other-worktree"));
        assert_eq!(
            reader.observe(
                &inspection.layout.active_index_path,
                &mismatched,
                MAX_INDEX_BYTES
            ),
            Err(IndexReadError::Unsupported)
        );
    }

    struct ReplacingIndexReader {
        inner: MacIndexFileReader,
        index_path: PathBuf,
        replaced: AtomicBool,
    }

    impl IndexFileReader for ReplacingIndexReader {
        fn observe(
            &self,
            path: &UntrustedGitPath,
            layout: &UntrustedGitLayout,
            maximum_bytes: u64,
        ) -> Result<IndexObservation, IndexReadError> {
            let observation = self.inner.observe(path, layout, maximum_bytes)?;
            if !self.replaced.swap(true, Ordering::SeqCst) {
                let bytes = fs::read(&self.index_path).unwrap();
                let replacement = self.index_path.with_extension("dcc-replacement");
                fs::write(&replacement, bytes).unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
                remove_xattrs(&replacement);
                fs::rename(&replacement, &self.index_path).unwrap();
                remove_xattrs(&self.index_path);
            }
            Ok(observation)
        }
    }

    #[test]
    fn system_git_inspection_detects_index_replacement_between_observations() {
        let fixture = Fixture::real_git_repository();
        let reader = ReplacingIndexReader {
            inner: MacIndexFileReader::new(fixture.root.clone(), fixture.workspace.clone())
                .unwrap(),
            index_path: fixture.directory.path().join(".git/index"),
            replaced: AtomicBool::new(false),
        };
        let git = TrustedGitBinary::verify_absolute(Path::new("/usr/bin/git")).unwrap();
        let inspector =
            GitInspector::with_index_reader(GitInspectorLimits::default(), git, reader).unwrap();
        assert_eq!(
            inspector.inspect(fixture.directory.path()).unwrap_err(),
            GitInspectorError::IndexChangedDuringInspection
        );
    }
}
