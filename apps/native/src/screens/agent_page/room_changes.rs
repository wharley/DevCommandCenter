//! The change card's files (`data::task_changes`), per task: read when a
//! card is drawn — so never for one off screen — off the UI thread, again
//! only when what can change them did (the report's totals, the rail's
//! branch, commits ahead and uncommitted files), at most once a minute for
//! a task at work, and kept for the last `KEPT` tasks. The uncommitted part
//! is the rail's own status, merged in when it moves, without a read.

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Context, Task};

use super::room::TeamRoom;
use crate::data::task_changes::{
    self, ChangedFile, ChangesRead, ChangesRequest, entries_fingerprint, fingerprint, from_entries,
    merge,
};
use crate::data::task_reports::TaskReport;

/// Tasks whose files are kept.
const KEPT: usize = 24;
/// A task at work is read again at most this often.
const RUNNING_EVERY: Duration = Duration::from_secs(60);

pub(crate) type Reader = fn(&ChangesRequest) -> ChangesRead;

#[derive(Default)]
struct Entry {
    /// What the last read was asked for.
    key: u64,
    read: Option<Rc<ChangesRead>>,
    /// Bumped by each read that lands, so the files merge again.
    seq: u64,
    read_at: Option<Instant>,
    reading: Option<Task<()>>,
    /// The files shown and what they were merged from.
    shown: Option<(u64, Rc<Vec<ChangedFile>>)>,
}

pub(crate) struct RoomChanges {
    entries: HashMap<String, Entry>,
    order: VecDeque<String>,
    /// Tasks whose list is open past three files.
    expanded: HashSet<String>,
    /// The read; tests put a fake here.
    pub(crate) reader: Reader,
}

impl Default for RoomChanges {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            expanded: HashSet::new(),
            reader: task_changes::read,
        }
    }
}

impl RoomChanges {
    fn entry(&mut self, id: &str) -> &mut Entry {
        if !self.entries.contains_key(id) {
            self.order.push_back(id.to_string());
            while self.order.len() > KEPT {
                if let Some(old) = self.order.pop_front() {
                    self.entries.remove(&old);
                    self.expanded.remove(&old);
                }
            }
        }
        self.entries.entry(id.to_string()).or_default()
    }

    fn settle(&mut self, id: &str, key: u64, read: ChangesRead) {
        if let Some(entry) = self.entries.get_mut(id) {
            entry.reading = None;
            entry.read_at = Some(Instant::now());
            // A newer key is read at the next draw.
            if entry.key == key {
                entry.read = Some(Rc::new(read));
                entry.seq += 1;
            }
        }
    }

    pub(crate) fn is_expanded(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// What the card draws for one task.
#[derive(Default)]
pub(crate) struct TaskChanges {
    /// None until the first read lands.
    pub files: Option<Rc<Vec<ChangedFile>>>,
    pub read: Option<Rc<ChangesRead>>,
}

/// Whether a read is due: never one in flight, and a task at work waits.
fn due(entry: &Entry, key: u64, turn_open: bool, now: Instant) -> bool {
    if entry.reading.is_some() {
        return false;
    }
    if entry.read_at.is_none() {
        return true;
    }
    entry.key != key
        && !(turn_open
            && entry
                .read_at
                .is_some_and(|at| now.duration_since(at) < RUNNING_EVERY))
}

impl TeamRoom {
    /// The files `report`'s task changed, reading them if they are due.
    pub(super) fn task_changes(&self, report: &TaskReport, cx: &mut Context<Self>) -> TaskChanges {
        let Some(id) = report.task_id.clone() else {
            return TaskChanges::default();
        };
        let (request, key, out) = {
            let sidebar = self.sidebar.read(cx);
            let checkout = sidebar.task_checkout(&id);
            let git = checkout
                .as_ref()
                .and_then(|checkout| checkout.snapshot)
                .and_then(|snapshot| snapshot.git.as_ref());
            let uncommitted = git
                .map(|git| entries_fingerprint(git.staged.iter().chain(&git.unstaged)))
                .unwrap_or(0);
            let key = fingerprint(&(
                (report.files, report.additions, report.deletions),
                checkout.as_ref().map(|checkout| checkout.root),
                git.map(|git| (git.current_branch.as_deref(), git.ahead_of_remote_count)),
                uncommitted,
            ));
            let turn_open = checkout.as_ref().is_some_and(|checkout| checkout.turn_open);
            let mut cache = self.changes.borrow_mut();
            let entry = cache.entry(&id);
            let request = due(entry, key, turn_open, Instant::now()).then(|| ChangesRequest {
                db_path: self.db_path.clone(),
                root: checkout.as_ref().map(|checkout| checkout.root.to_string()),
                session_id: report.session_id.clone(),
                needs_status: git.is_none(),
            });
            if request.is_some() {
                entry.key = key;
            }
            if let Some(read) = &entry.read {
                let merged = fingerprint(&(entry.seq, uncommitted));
                if entry.shown.as_ref().map(|(at, _)| *at) != Some(merged) {
                    let live = git.map_or_else(Vec::new, |git| {
                        from_entries(git.staged.iter().chain(&git.unstaged))
                    });
                    let files = merge(read.committed.iter().chain(&read.uncommitted).chain(&live));
                    entry.shown = Some((merged, Rc::new(files)));
                }
            }
            let out = TaskChanges {
                files: entry.shown.as_ref().map(|(_, files)| files.clone()),
                read: entry.read.clone(),
            };
            (request, key, out)
        };
        if let Some(request) = request {
            let reader = self.changes.borrow().reader;
            let task_id = id.clone();
            let task = cx.spawn(async move |this, cx| {
                let read = cx
                    .background_executor()
                    .spawn(async move { reader(&request) })
                    .await;
                let _ = this.update(cx, |room, cx| {
                    room.changes.borrow_mut().settle(&task_id, key, read);
                    cx.notify();
                });
            });
            if let Some(entry) = self.changes.borrow_mut().entries.get_mut(&id) {
                entry.reading = Some(task);
            }
        }
        out
    }

    /// "mais N arquivos" and "Mostrar menos".
    pub(super) fn toggle_changes(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let mut cache = self.changes.borrow_mut();
        if !cache.expanded.remove(task_id) {
            cache.expanded.insert(task_id.to_string());
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_is_due_once_per_change_and_a_task_at_work_waits() {
        let now = Instant::now();
        let fresh = Entry::default();
        assert!(due(&fresh, 1, false, now), "never read");
        let read = Entry {
            key: 1,
            read_at: Some(now),
            ..Entry::default()
        };
        assert!(!due(&read, 1, false, now), "same key");
        assert!(due(&read, 2, false, now), "the changes moved");
        assert!(!due(&read, 2, true, now), "at work: a minute between reads");
        assert!(due(&read, 2, true, now + RUNNING_EVERY));
    }

    #[test]
    fn the_cache_keeps_the_last_tasks() {
        let mut cache = RoomChanges::default();
        for n in 0..(KEPT + 5) {
            cache.entry(&format!("t-{n}"));
        }
        assert!(!cache.entries.contains_key("t-0"), "the oldest went first");
        // The oldest left is the next to go, its list folded with it.
        cache.expanded.insert("t-5".into());
        cache.entry("t-new");
        assert_eq!(cache.len(), KEPT);
        assert!(!cache.entries.contains_key("t-5"));
        assert!(!cache.is_expanded("t-5"));
        assert!(cache.entries.contains_key("t-new"));
    }
}
