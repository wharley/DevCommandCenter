//! Background pass that merges the streamed chunks of settled turns (see
//! `dcc_infra::db::delta_compaction`) and then gives free pages back to the
//! disk. It runs on its own connection, in small write transactions with
//! pauses between them, so agents appending events never wait on it for long.

use std::path::PathBuf;
use std::time::Duration;

use dcc_infra::db::SqliteSessionRepo;

/// Startup is busy enough; the first pass waits.
const FIRST_PASS_DELAY: Duration = Duration::from_secs(120);
const PASS_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Rows deleted per write transaction, and the pause after each one.
const ROWS_PER_STEP: usize = 1000;
const PAUSE_BETWEEN_STEPS: Duration = Duration::from_millis(100);
/// Free pages (4 KB) given back to the disk per write, after each pass.
const PAGES_PER_RECLAIM: u32 = 2048;

/// One full pass; also what "Compactar" in the cleanup dialog runs.
pub(crate) fn run_pass(db_path: &PathBuf) -> Result<(usize, usize), String> {
    let repo = SqliteSessionRepo::open(db_path).map_err(|error| error.to_string())?;
    let (mut merged, mut deleted) = (0, 0);
    for session_id in repo
        .list_sessions_pending_delta_compaction_sync()
        .map_err(|error| error.to_string())?
    {
        loop {
            let step = repo
                .compact_session_deltas_step(&session_id, chrono::Utc::now(), ROWS_PER_STEP)
                .map_err(|error| error.to_string())?;
            merged += step.merged_rows;
            deleted += step.deleted_rows;
            if step.deleted_rows > 0 {
                std::thread::sleep(PAUSE_BETWEEN_STEPS);
            }
            if !step.more {
                break;
            }
        }
    }
    // Freed pages, from this pass or from deleted tasks, go back to the disk;
    // stop as soon as a slice makes no progress.
    let mut free_pages = i64::MAX;
    loop {
        let remaining = repo
            .reclaim_free_pages_step(PAGES_PER_RECLAIM)
            .map_err(|error| error.to_string())?;
        if remaining == 0 || remaining >= free_pages {
            break;
        }
        free_pages = remaining;
        std::thread::sleep(PAUSE_BETWEEN_STEPS);
    }
    Ok((merged, deleted))
}

pub fn spawn(db_path: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("dcc-delta-compaction".into())
        .spawn(move || {
            std::thread::sleep(FIRST_PASS_DELAY);
            loop {
                match run_pass(&db_path) {
                    Ok((_, 0)) => {}
                    Ok((merged, deleted)) => eprintln!(
                        "[DCC] streamed chunks compacted: {deleted} rows merged into {merged}"
                    ),
                    Err(error) => eprintln!("[DCC] delta compaction skipped: {error}"),
                }
                std::thread::sleep(PASS_INTERVAL);
            }
        });
    if let Err(error) = spawned {
        eprintln!("[DCC] delta compaction unavailable: {error}");
    }
}
