//! Which process owns the runtime of a DCC data directory: the agents, the
//! terminals and the in-memory git coordinator. The desktop holds this lock
//! for as long as it runs; another app (the native one) changes git or
//! deletes tasks only while it holds the lock itself, so the two never
//! mutate the same checkouts at once. The OS drops the lock with the process.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const FILE_NAME: &str = "runtime-owner.lock";
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct RuntimeOwner {
    _file: File,
}

impl RuntimeOwner {
    /// The lock if no other process holds it right now.
    pub fn try_acquire(app_data_dir: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(app_data_dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(app_data_dir.join(FILE_NAME))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }

    /// Waits up to `timeout` for a short holder (a deletion in flight) to finish.
    pub fn acquire_within(app_data_dir: &Path, timeout: Duration) -> io::Result<Option<Self>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(owner) = Self::try_acquire(app_data_dir)? {
                return Ok(Some(owner));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(POLL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeOwner;
    use std::time::Duration;

    #[test]
    fn one_owner_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = RuntimeOwner::try_acquire(dir.path()).unwrap();
        assert!(first.is_some());
        assert!(RuntimeOwner::try_acquire(dir.path()).unwrap().is_none());
        assert!(
            RuntimeOwner::acquire_within(dir.path(), Duration::from_millis(150))
                .unwrap()
                .is_none()
        );
        drop(first);
        // A child another test is spawning may hold the lock a moment, until
        // it execs (flock follows the open file it inherited).
        assert!(
            RuntimeOwner::acquire_within(dir.path(), Duration::from_secs(1))
                .unwrap()
                .is_some()
        );
    }
}
