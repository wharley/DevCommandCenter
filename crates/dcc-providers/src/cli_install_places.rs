//! Where agents' CLIs live beyond the login shell's PATH, per platform. On
//! the Mac that is Homebrew's two prefixes; on Linux, Homebrew lives under
//! `/home/linuxbrew` (or `~/.linuxbrew`), and a global npm is often a
//! user prefix (`~/.npm-global`), Volta, Bun or pnpm — or the distro's
//! Node under `/usr`, which only root can write.

use std::path::{Path, PathBuf};

/// Homebrew's `bin` directories on this platform, most usual first.
pub(crate) fn homebrew_bins(home: Option<&str>) -> Vec<PathBuf> {
    homebrew_bins_for(cfg!(target_os = "linux"), home)
}

fn homebrew_bins_for(linux: bool, home: Option<&str>) -> Vec<PathBuf> {
    if linux {
        std::iter::once(PathBuf::from("/home/linuxbrew/.linuxbrew/bin"))
            .chain(home.map(|home| Path::new(home).join(".linuxbrew/bin")))
            .collect()
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    }
}

/// Directories a CLI may be installed in that a desktop session's PATH can
/// lack: tried after the PATH (only on Linux: the Mac app reads the login
/// shell's PATH and has always worked without them).
pub(crate) fn extra_bin_dirs(home: &str) -> Vec<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let pnpm = std::env::var("PNPM_HOME")
        .ok()
        .filter(|dir| !dir.is_empty());
    linux_user_bins(home, pnpm.as_deref())
        .into_iter()
        .chain(homebrew_bins_for(true, Some(home)))
        .filter(|dir| dir.is_dir())
        .collect()
}

fn linux_user_bins(home: &str, pnpm_home: Option<&str>) -> Vec<PathBuf> {
    let home = Path::new(home);
    [
        ".npm-global/bin",
        ".npm-packages/bin",
        ".volta/bin",
        ".bun/bin",
        ".local/share/pnpm",
    ]
    .into_iter()
    .map(|dir| home.join(dir))
    .chain(pnpm_home.map(PathBuf::from))
    .collect()
}

/// Whether this process may write `dir` (a global npm under `/usr` cannot be
/// updated without `sudo`, which only a terminal the person watches asks).
#[cfg(unix)]
pub(crate) fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a valid NUL-terminated string for the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

#[cfg(not(unix))]
pub(crate) fn writable(_dir: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homebrew_lives_where_each_platform_puts_it() {
        assert_eq!(
            homebrew_bins_for(false, Some("/Users/w")),
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin")
            ]
        );
        assert_eq!(
            homebrew_bins_for(true, Some("/home/w")),
            [
                PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
                PathBuf::from("/home/w/.linuxbrew/bin")
            ]
        );
    }

    #[test]
    fn linux_user_prefixes_include_pnpm_home() {
        let dirs = linux_user_bins("/home/w", Some("/opt/pnpm"));
        assert!(dirs.contains(&PathBuf::from("/home/w/.npm-global/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/w/.volta/bin")));
        assert_eq!(dirs.last(), Some(&PathBuf::from("/opt/pnpm")));
    }

    #[cfg(unix)]
    #[test]
    fn a_root_owned_directory_is_not_writable() {
        assert!(writable(&std::env::temp_dir()));
        if unsafe { libc::geteuid() } != 0 {
            assert!(!writable(Path::new("/usr/lib")) || !Path::new("/usr/lib").exists());
        }
    }
}
