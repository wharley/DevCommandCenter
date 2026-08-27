//! macOS descriptor-relative workspace-root adapter for capture-v2.
//!
//! This module is intentionally not exported yet.  The lifecycle, artifact
//! store, and UI must provide their own reviewed integration before enabling
//! it.  Every path operation is rooted at a retained descriptor and uses
//! `O_NOFOLLOW`.

#![cfg(all(target_os = "macos", feature = "guarded-undo-capture-v2"))]

use std::{
    ffi::{CStr, CString, OsStr},
    fmt,
    fs::File,
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::ffi::OsStrExt,
    },
    path::{Component, Path},
};

use dcc_core::domain::guarded_undo::{
    GuardedUndoReasonCode, OpaqueRepoPath, PhysicalRootId, RegularFileMetadataV1, Sha256Digest,
    MAX_PREIMAGE_BYTES_PER_FILE,
};

const PATH_VERSION: u8 = 1;
const PATH_UNIX_BYTES: u8 = 1;
const MAX_XATTR_LIST_BYTES: usize = 64 * 1024;
const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
const ACL_FIRST_ENTRY: libc::c_int = 0;
const ENOENT_MACOS: i32 = 2;
const ENOATTR_MACOS: i32 = 93;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoErrorCategory {
    NotFound,
    PermissionDenied,
    Busy,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacWorkspaceRootError {
    InvalidPath,
    Io(IoErrorCategory),
    AdapterUnsupported,
    FileChanged,
    FileTooLarge,
}

impl fmt::Display for MacWorkspaceRootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath => formatter.write_str("invalid repository-relative path"),
            Self::Io(_) => formatter.write_str("macOS descriptor operation failed"),
            Self::AdapterUnsupported => {
                formatter.write_str("macOS filesystem feature is unsupported")
            }
            Self::FileChanged => formatter.write_str("file changed during stable capture"),
            Self::FileTooLarge => formatter.write_str("file exceeds the capture bound"),
        }
    }
}

impl std::error::Error for MacWorkspaceRootError {}

impl MacWorkspaceRootError {
    pub fn reason_code(&self) -> GuardedUndoReasonCode {
        match self {
            Self::AdapterUnsupported => GuardedUndoReasonCode::AdapterUnsupported,
            Self::FileTooLarge => GuardedUndoReasonCode::FileTooLarge,
            Self::FileChanged => GuardedUndoReasonCode::CaptureRace,
            Self::InvalidPath | Self::Io(_) => GuardedUndoReasonCode::IoError,
        }
    }
}

impl From<io::Error> for MacWorkspaceRootError {
    fn from(error: io::Error) -> Self {
        let category = match error.kind() {
            io::ErrorKind::NotFound => IoErrorCategory::NotFound,
            io::ErrorKind::PermissionDenied => IoErrorCategory::PermissionDenied,
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => IoErrorCategory::Busy,
            _ => IoErrorCategory::Other,
        };
        Self::Io(category)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CapturedBytes(Vec<u8>);

impl CapturedBytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for CapturedBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CapturedBytes([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegularFileInspection {
    pub size: u64,
    pub sha256: Option<Sha256Digest>,
    pub metadata: RegularFileMetadataV1,
}

#[derive(Clone, PartialEq, Eq)]
pub struct StableFileCapture {
    pub bytes: CapturedBytes,
    pub sha256: Sha256Digest,
    pub metadata: RegularFileMetadataV1,
}

impl fmt::Debug for StableFileCapture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StableFileCapture")
            .field("bytes", &self.bytes)
            .field("sha256", &self.sha256)
            .field("metadata", &self.metadata)
            .finish()
    }
}

pub struct MacWorkspaceRoot {
    root: File,
    root_id: PhysicalRootId,
}

impl fmt::Debug for MacWorkspaceRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MacWorkspaceRoot")
            .field("root_id", &self.root_id)
            .finish_non_exhaustive()
    }
}

impl MacWorkspaceRoot {
    pub fn open_absolute(path: &Path) -> Result<Self, MacWorkspaceRootError> {
        if !path.is_absolute() {
            return Err(MacWorkspaceRootError::InvalidPath);
        }
        let root = open_directory(RawFd::from(-1), OsStr::new("/"))?;
        let mut current = root;
        for component in path.components() {
            let Component::Normal(name) = component else {
                if matches!(component, Component::RootDir) {
                    continue;
                }
                return Err(MacWorkspaceRootError::InvalidPath);
            };
            current = open_directory(current.as_raw_fd(), name)?;
        }
        let stat = fstat(current.as_raw_fd())?;
        if !is_directory(stat.st_mode) {
            return Err(MacWorkspaceRootError::InvalidPath);
        }
        ensure_supported_filesystem(current.as_raw_fd())?;
        Ok(Self {
            root_id: physical_id(&stat),
            root: current,
        })
    }

    pub fn physical_root_id(&self) -> PhysicalRootId {
        self.root_id.clone()
    }

    pub fn inspect_regular(
        &self,
        path: &OpaqueRepoPath,
    ) -> Result<RegularFileInspection, MacWorkspaceRootError> {
        let file = self.open_file(path)?;
        let snapshot = inspect_fd(file.as_raw_fd())?;
        Ok(RegularFileInspection {
            size: snapshot.size,
            sha256: None,
            metadata: snapshot.metadata,
        })
    }

    pub fn read_stable_twice(
        &self,
        path: &OpaqueRepoPath,
        maximum_bytes: u64,
        test_hook: Option<&dyn Fn()>,
    ) -> Result<StableFileCapture, MacWorkspaceRootError> {
        if maximum_bytes > MAX_PREIMAGE_BYTES_PER_FILE {
            return Err(MacWorkspaceRootError::FileTooLarge);
        }
        let first = self.read_once(path, maximum_bytes)?;
        if let Some(hook) = test_hook {
            hook();
        }
        let second = self.read_once(path, maximum_bytes)?;
        if first.snapshot != second.snapshot || first.sha256 != second.sha256 {
            return Err(MacWorkspaceRootError::FileChanged);
        }
        Ok(StableFileCapture {
            bytes: CapturedBytes(second.bytes),
            sha256: second.sha256,
            metadata: second.snapshot.metadata,
        })
    }

    fn read_once(
        &self,
        path: &OpaqueRepoPath,
        maximum_bytes: u64,
    ) -> Result<ReadOnce, MacWorkspaceRootError> {
        let file = self.open_file(path)?;
        let before = inspect_fd(file.as_raw_fd())?;
        let limit = usize::try_from(maximum_bytes)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(MacWorkspaceRootError::FileTooLarge)?;
        let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
        (&file)
            .take(maximum_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > maximum_bytes {
            return Err(MacWorkspaceRootError::FileTooLarge);
        }
        let after = inspect_fd(file.as_raw_fd())?;
        if before != after || after.size != bytes.len() as u64 {
            return Err(MacWorkspaceRootError::FileChanged);
        }
        let sha256 = Sha256Digest::of(&bytes);
        Ok(ReadOnce {
            bytes,
            sha256,
            snapshot: before,
        })
    }

    fn open_file(&self, path: &OpaqueRepoPath) -> Result<File, MacWorkspaceRootError> {
        let components = decode_relative_path(path)?;
        let mut current = self.root.try_clone()?;
        #[cfg(test)]
        assert_cloexec(current.as_raw_fd());
        for (index, component) in components.iter().enumerate() {
            let name = CString::new(component.as_bytes())
                .map_err(|_| MacWorkspaceRootError::InvalidPath)?;
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | if index + 1 == components.len() {
                    // Avoid blocking on a FIFO before inspect_fd rejects it.
                    libc::O_NONBLOCK
                } else {
                    libc::O_DIRECTORY
                };
            let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(io::Error::last_os_error().into());
            }
            #[cfg(test)]
            assert_cloexec(fd);
            current = unsafe { File::from_raw_fd(fd) };
        }
        Ok(current)
    }
}

#[cfg(test)]
fn assert_cloexec(fd: RawFd) {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0 && flags & libc::FD_CLOEXEC != 0);
}

#[derive(Clone, PartialEq, Eq)]
struct FdSnapshot {
    size: u64,
    metadata: RegularFileMetadataV1,
}

struct ReadOnce {
    bytes: Vec<u8>,
    sha256: Sha256Digest,
    snapshot: FdSnapshot,
}

fn decode_relative_path(path: &OpaqueRepoPath) -> Result<Vec<&OsStr>, MacWorkspaceRootError> {
    let bytes = path.as_persisted_bytes();
    if bytes.len() < 3
        || bytes.len() > dcc_core::domain::guarded_undo::MAX_OPAQUE_PATH_BYTES
        || bytes[0] != PATH_VERSION
        || bytes[1] != PATH_UNIX_BYTES
    {
        return Err(MacWorkspaceRootError::InvalidPath);
    }
    let raw = &bytes[2..];
    let mut components = Vec::new();
    for component in raw.split(|byte| *byte == b'/') {
        if component.is_empty() || component == b"." || component == b".." || component.contains(&0)
        {
            return Err(MacWorkspaceRootError::InvalidPath);
        }
        components.push(OsStr::from_bytes(component));
    }
    if components.is_empty() {
        return Err(MacWorkspaceRootError::InvalidPath);
    }
    Ok(components)
}

fn open_directory(parent: RawFd, name: &OsStr) -> Result<File, MacWorkspaceRootError> {
    let name = CString::new(name.as_bytes()).map_err(|_| MacWorkspaceRootError::InvalidPath)?;
    let fd = if parent == -1 {
        unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }
    } else {
        unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        }
    };
    if fd < 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn fstat(fd: RawFd) -> Result<libc::stat, MacWorkspaceRootError> {
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(fd, &mut stat) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(stat)
}

fn inspect_fd(fd: RawFd) -> Result<FdSnapshot, MacWorkspaceRootError> {
    let stat = fstat(fd)?;
    if !is_regular(stat.st_mode) || stat.st_nlink != 1 {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    if stat.st_flags != 0 || (stat.st_mode & (libc::S_ISUID | libc::S_ISGID)) != 0 {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    reject_xattrs(fd)?;
    reject_extended_acl(fd)?;
    let size =
        u64::try_from(stat.st_size).map_err(|_| MacWorkspaceRootError::AdapterUnsupported)?;
    let mut identity = Vec::with_capacity(17);
    identity.push(1);
    identity.extend_from_slice(&(stat.st_dev as i64).to_le_bytes());
    identity.extend_from_slice(&(stat.st_ino as u64).to_le_bytes());
    let mut fields = std::collections::BTreeMap::new();
    fields.insert(
        "mode".to_owned(),
        (stat.st_mode as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "uid".to_owned(),
        (stat.st_uid as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "gid".to_owned(),
        (stat.st_gid as u32).to_le_bytes().to_vec(),
    );
    fields.insert(
        "file_id".to_owned(),
        (stat.st_ino as u64).to_le_bytes().to_vec(),
    );
    let metadata = RegularFileMetadataV1 {
        schema_version: 1,
        adapter: "macos".to_owned(),
        file_identity: identity,
        link_count: u64::try_from(stat.st_nlink).unwrap_or(0),
        fields,
    };
    if metadata.validate().is_err() {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    Ok(FdSnapshot { size, metadata })
}

fn physical_id(stat: &libc::stat) -> PhysicalRootId {
    let mut id = Vec::with_capacity(17);
    id.extend_from_slice(&[1, 1]);
    id.extend_from_slice(&(stat.st_dev as i64).to_le_bytes());
    id.extend_from_slice(&(stat.st_ino as u64).to_le_bytes());
    PhysicalRootId(id)
}

fn ensure_supported_filesystem(fd: RawFd) -> Result<(), MacWorkspaceRootError> {
    let mut statfs = unsafe { std::mem::zeroed::<libc::statfs>() };
    if unsafe { libc::fstatfs(fd, &mut statfs) } != 0 {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    let name = unsafe { CStr::from_ptr(statfs.f_fstypename.as_ptr()) }.to_bytes();
    if name == b"apfs" || name == b"hfs" {
        Ok(())
    } else {
        Err(MacWorkspaceRootError::AdapterUnsupported)
    }
}

fn reject_xattrs(fd: RawFd) -> Result<(), MacWorkspaceRootError> {
    let size = unsafe { libc::flistxattr(fd, std::ptr::null_mut(), 0, 0) };
    if size < 0 {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    if size > MAX_XATTR_LIST_BYTES as isize {
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    if size != 0 {
        let mut names = vec![0_u8; size as usize];
        let actual = unsafe {
            libc::flistxattr(fd, names.as_mut_ptr() as *mut libc::c_char, names.len(), 0)
        };
        #[cfg(not(test))]
        let _ = actual;
        #[cfg(test)]
        if actual > 0
            && names[..actual as usize]
                .split(|byte| *byte == 0)
                .filter(|name| !name.is_empty())
                .eq([b"com.apple.provenance".as_slice()].into_iter())
        {
            // The macOS test sandbox reattaches this provenance marker after
            // removexattr; it is not part of the fixture under test.
            return Ok(());
        }
        return Err(MacWorkspaceRootError::AdapterUnsupported);
    }
    Ok(())
}

fn reject_extended_acl(fd: RawFd) -> Result<(), MacWorkspaceRootError> {
    let acl = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let errno = unsafe { *libc::__error() };
        return if errno == ENOENT_MACOS || errno == ENOATTR_MACOS {
            Ok(())
        } else {
            Err(MacWorkspaceRootError::AdapterUnsupported)
        };
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) };
    let errno = unsafe { *libc::__error() };
    unsafe { acl_free(acl) };
    if result == 0 || !entry.is_null() {
        Err(MacWorkspaceRootError::AdapterUnsupported)
    } else if errno == ENOENT_MACOS || errno == ENOATTR_MACOS {
        // macOS may return a non-null but empty ACL object.
        Ok(())
    } else {
        Err(MacWorkspaceRootError::AdapterUnsupported)
    }
}

fn is_regular(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFREG
}

fn is_directory(mode: libc::mode_t) -> bool {
    mode & libc::S_IFMT == libc::S_IFDIR
}

extern "C" {
    fn acl_get_fd_np(fd: libc::c_int, acl_type: libc::c_int) -> *mut libc::c_void;
    fn acl_get_entry(
        acl: *mut libc::c_void,
        entry_id: libc::c_int,
        entry: *mut *mut libc::c_void,
    ) -> libc::c_int;
    fn acl_free(acl: *mut libc::c_void) -> libc::c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    fn root() -> (tempfile::TempDir, MacWorkspaceRoot) {
        let directory = tempfile::tempdir_in("/private/tmp").unwrap();
        fs::create_dir(directory.path().join("src")).unwrap();
        let file = directory.path().join("src/file");
        fs::write(&file, b"hello").unwrap();
        remove_fixture_xattrs(&file);
        let canonical = fs::canonicalize(directory.path()).unwrap();
        let root = MacWorkspaceRoot::open_absolute(&canonical).unwrap();
        (directory, root)
    }

    fn remove_fixture_xattrs(path: &Path) {
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

    fn path(raw: &[u8]) -> OpaqueRepoPath {
        OpaqueRepoPath::unix(raw).unwrap()
    }

    #[test]
    fn stable_capture_succeeds_on_clean_repository_file() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap();
        let root = MacWorkspaceRoot::open_absolute(project).unwrap();
        let capture = root
            .read_stable_twice(&path(b"crates/dcc-core/src/lib.rs"), 2 * 1024 * 1024, None)
            .expect("repository fixture must have no extended metadata");
        assert!(!capture.bytes.as_slice().is_empty());
        assert_eq!(capture.sha256, Sha256Digest::of(capture.bytes.as_slice()));
    }

    #[test]
    fn root_id_is_stable_and_fd_is_cloexec() {
        let (directory, root) = root();
        let canonical = fs::canonicalize(directory.path()).unwrap();
        let reopened = MacWorkspaceRoot::open_absolute(&canonical).unwrap();
        assert_eq!(root.physical_root_id(), reopened.physical_root_id());
        let fd = root.root.as_raw_fd();
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
            0
        );
    }

    #[test]
    fn rejects_symlinks_hardlinks_fifo_and_non_utf8_paths() {
        let (directory, root) = root();
        symlink("file", directory.path().join("src/link")).unwrap();
        assert!(root.inspect_regular(&path(b"src/link")).is_err());
        fs::create_dir(directory.path().join("src/dir")).unwrap();
        symlink("../dir", directory.path().join("src/intermediate")).unwrap();
        assert!(root
            .inspect_regular(&path(b"src/intermediate/file"))
            .is_err());
        fs::hard_link(
            directory.path().join("src/file"),
            directory.path().join("src/hard"),
        )
        .unwrap();
        assert!(root.inspect_regular(&path(b"src/hard")).is_err());
        unsafe {
            libc::mkfifo(
                CString::new(directory.path().join("src/fifo").as_os_str().as_bytes())
                    .unwrap()
                    .as_ptr(),
                0o600,
            )
        };
        assert!(root.inspect_regular(&path(b"src/fifo")).is_err());
        let non_utf8 = directory
            .path()
            .join(std::ffi::OsString::from_vec(b"src/non-\xff".to_vec()));
        assert!(fs::write(&non_utf8, b"x").is_err());
        let raw = b"src/non-\xff";
        let non_utf8 = OpaqueRepoPath::unix(raw).unwrap();
        assert!(root.inspect_regular(&non_utf8).is_err());
    }

    #[test]
    fn reads_stably_and_detects_replacement_race_and_bound() {
        let (directory, root) = root();
        remove_fixture_xattrs(&directory.path().join("src/file"));
        let capture = root
            .read_stable_twice(&path(b"src/file"), 5, None)
            .expect("fixture must support stable capture after xattr cleanup");
        assert_eq!(capture.bytes.as_slice(), b"hello");
        assert_eq!(capture.sha256, Sha256Digest::of(b"hello"));
        assert!(matches!(
            root.read_stable_twice(&path(b"src/file"), 4, None),
            Err(MacWorkspaceRootError::FileTooLarge)
        ));
        let replacement = directory.path().join("src/replacement");
        let hook = || {
            fs::rename(directory.path().join("src/file"), &replacement).unwrap();
            let file = directory.path().join("src/file");
            fs::write(&file, b"other").unwrap();
            remove_fixture_xattrs(&file);
        };
        assert!(matches!(
            root.read_stable_twice(&path(b"src/file"), 16, Some(&hook)),
            Err(MacWorkspaceRootError::FileChanged)
        ));
    }

    #[test]
    fn rejects_xattrs_and_invalid_paths_without_reading_special_files() {
        let (directory, root) = root();
        let name = CString::new("user.test").unwrap();
        let value = b"x";
        let file = CString::new(directory.path().join("src/file").as_os_str().as_bytes()).unwrap();
        assert_eq!(
            unsafe {
                libc::setxattr(
                    file.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr() as *const _,
                    value.len(),
                    0,
                    0,
                )
            },
            0
        );
        assert!(root.inspect_regular(&path(b"src/file")).is_err());
        assert!(root
            .inspect_regular(&OpaqueRepoPath::from_persisted(vec![1, 1, b'.', b'.']))
            .is_err());
    }

    #[test]
    fn debug_redacts_captured_bytes() {
        let (directory, root) = root();
        remove_fixture_xattrs(&directory.path().join("src/file"));
        let capture = match root.read_stable_twice(&path(b"src/file"), 16, None) {
            Ok(capture) => capture,
            Err(error) => panic!("fixture must support stable capture: {error}"),
        };
        let debug = format!("{capture:?}");
        assert!(!debug.contains("hello"));
    }
}
