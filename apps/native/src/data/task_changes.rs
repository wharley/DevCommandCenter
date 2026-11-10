//! The files a task changed, for the room's change card (`room_changes`):
//! what its branch committed against the base plus what is still
//! uncommitted, per file, the biggest first. The read is the one "Mudanças"
//! uses (the branch diff; the uncommitted part comes from the rail's own
//! status); a task whose checkout is gone falls back to its turns' change
//! sets, which DCC keeps. Blocking: `read` runs off the UI thread.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use dcc_core::domain::workspace_git::WorkspaceGitChangeEntry;

/// Files listed before "mais N arquivos".
pub const SHOWN: usize = 3;
/// Files listed once opened; the rest is in "Mudanças".
pub const EXPANDED_CAP: usize = 30;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChangedFile {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

/// What a read found: the branch's commits (or the turns' captures), and
/// the uncommitted files when the rail has no status for the task.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChangesRead {
    pub committed: Vec<ChangedFile>,
    pub uncommitted: Vec<ChangedFile>,
    /// From the checkout (git), not from the captures.
    pub from_checkout: bool,
}

/// What to read for one task.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangesRequest {
    pub db_path: PathBuf,
    /// The task's checkout, when the sidebar knows it.
    pub root: Option<String>,
    /// The task's session, for its turns' captures.
    pub session_id: Option<String>,
    /// The rail has no status for it (a closed task): read it here.
    pub needs_status: bool,
}

pub fn from_entries<'a>(
    entries: impl IntoIterator<Item = &'a WorkspaceGitChangeEntry>,
) -> Vec<ChangedFile> {
    entries
        .into_iter()
        .map(|entry| ChangedFile {
            path: entry.path.clone(),
            additions: u64::from(entry.insertions),
            deletions: u64::from(entry.deletions),
        })
        .collect()
}

/// One row per path, its lines summed across the sources; the biggest
/// change first, then by path.
pub fn merge<'a>(sources: impl IntoIterator<Item = &'a ChangedFile>) -> Vec<ChangedFile> {
    let mut by_path: HashMap<&str, (u64, u64)> = HashMap::new();
    for file in sources {
        let lines = by_path.entry(file.path.as_str()).or_default();
        lines.0 += file.additions;
        lines.1 += file.deletions;
    }
    let mut files: Vec<ChangedFile> = by_path
        .into_iter()
        .map(|(path, (additions, deletions))| ChangedFile {
            path: path.to_string(),
            additions,
            deletions,
        })
        .collect();
    files.sort_by(|a, b| {
        (b.additions + b.deletions)
            .cmp(&(a.additions + a.deletions))
            .then_with(|| a.path.cmp(&b.path))
    });
    files
}

/// How many rows show and how many are left: three folded, up to
/// `EXPANDED_CAP` opened.
pub fn visible(total: usize, expanded: bool) -> (usize, usize) {
    let shown = total.min(if expanded { EXPANDED_CAP } else { SHOWN });
    (shown, total - shown)
}

/// A cheap fingerprint of what can change a list, so the read runs again
/// only when it did.
pub fn fingerprint<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// The uncommitted entries' fingerprint (paths and lines).
pub fn entries_fingerprint<'a>(
    entries: impl IntoIterator<Item = &'a WorkspaceGitChangeEntry>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry in entries {
        (&entry.path, entry.insertions, entry.deletions).hash(&mut hasher);
    }
    hasher.finish()
}

/// The read itself. Blocking: git runs for a checkout that is there, the
/// database for one that is gone; nothing without either.
pub fn read(request: &ChangesRequest) -> ChangesRead {
    if let Some(root) = request
        .root
        .as_deref()
        .filter(|root| Path::new(root).is_dir())
    {
        let committed = crate::data::git_watch::run_blocking(
            dcc_infra::workspace_git::workspace_git_branch_diff(&request.db_path, root),
        )
        .map(|diff| from_entries(&diff.changes))
        .unwrap_or_default();
        let uncommitted = if request.needs_status {
            dcc_infra::workspace_git::read_rail_git_status(root)
                .map(|status| from_entries(status.staged.iter().chain(&status.unstaged)))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        return ChangesRead {
            committed,
            uncommitted,
            from_checkout: true,
        };
    }
    // Never DCC's own database from a test, which leaves the path empty.
    let Some(session) = request.session_id.as_deref() else {
        return ChangesRead::default();
    };
    if request.db_path.as_os_str().is_empty() {
        return ChangesRead::default();
    }
    ChangesRead {
        committed: captured(&request.db_path, session),
        ..ChangesRead::default()
    }
}

/// The session's turns' captures, summed by file.
fn captured(db_path: &Path, session: &str) -> Vec<ChangedFile> {
    let Ok(repo) = dcc_infra::db::SqliteSessionRepo::open_read_only(db_path) else {
        return Vec::new();
    };
    let session = dcc_core::domain::session::SessionId(session.to_string());
    let Ok(sets) = repo.list_turn_change_sets_by_session(&session) else {
        return Vec::new();
    };
    let files: Vec<ChangedFile> = sets
        .iter()
        .flat_map(|set| &set.files)
        .map(|file| ChangedFile {
            path: file.path.clone(),
            additions: u64::from(file.insertions),
            deletions: u64::from(file.deletions),
        })
        .collect();
    merge(&files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, additions: u64, deletions: u64) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            additions,
            deletions,
        }
    }

    #[test]
    fn the_same_file_is_one_row_and_the_biggest_comes_first() {
        let committed = [file("src/a.rs", 10, 2), file("README.md", 1, 0)];
        let uncommitted = [file("src/a.rs", 3, 1), file("src/b.rs", 40, 0)];
        assert_eq!(
            merge(committed.iter().chain(&uncommitted)),
            [
                file("src/b.rs", 40, 0),
                file("src/a.rs", 13, 3),
                file("README.md", 1, 0)
            ]
        );
    }

    #[test]
    fn three_show_folded_and_thirty_opened() {
        assert_eq!(visible(2, false), (2, 0));
        assert_eq!(visible(7, false), (3, 4));
        assert_eq!(visible(7, true), (7, 0));
        assert_eq!(visible(45, true), (30, 15));
        assert_eq!(visible(0, true), (0, 0));
    }

    #[test]
    fn nothing_is_read_without_a_checkout_or_a_database() {
        let request = ChangesRequest {
            db_path: PathBuf::new(),
            root: Some("/nonexistent/dcc-task".into()),
            session_id: Some("s".into()),
            needs_status: true,
        };
        assert_eq!(read(&request), ChangesRead::default());
    }
}
