//! What the completed tasks take on disk (`useCompletedCleanupScan`):
//! measured off the UI thread once per set of completed tasks, again after
//! ten minutes. The header shows the total; deleting a task shows its share.

use std::time::{Duration, Instant};

use gpui::Context;

use super::Sidebar;
use crate::data::deletion;
use crate::data::disk::disk_usage_ids;
use crate::data::loader;
use crate::data::rail;
use crate::data::tasks::TaskSummary;
use dcc_infra::workspace_cleanup::WorkspaceCleanupScanOutput;

const STALE: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct CompletedScan {
    key: Vec<String>,
    pub output: Option<WorkspaceCleanupScanOutput>,
    scanned_at: Option<Instant>,
    running: bool,
    /// The last pass failed (the header shows nothing).
    pub failed: bool,
}

impl Sidebar {
    /// The completed tasks, as the "Concluídos" section lists them.
    pub(super) fn completed_tasks(&self) -> Vec<TaskSummary> {
        rail::split(&self.state.data.tasks, self.state.now_ms).completed
    }

    pub(super) fn refresh_completed_scan(&mut self, cx: &mut Context<Self>) {
        let completed = self.completed_tasks();
        let mut key = disk_usage_ids(&completed.iter().collect::<Vec<_>>());
        key.sort();
        let scan = &mut self.completed_scan;
        if key.is_empty() {
            *scan = CompletedScan::default();
            return;
        }
        let fresh = scan.key == key && scan.scanned_at.is_some_and(|at| at.elapsed() < STALE);
        if fresh || scan.running {
            return;
        }
        scan.running = true;
        let ids = key.clone();
        let db_path = loader::database_path();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { deletion::cleanup_scan(&db_path, ids) })
                .await;
            let _ = this.update(cx, |sidebar, cx| {
                let scan = &mut sidebar.completed_scan;
                scan.running = false;
                scan.scanned_at = Some(Instant::now());
                scan.key = key;
                match result {
                    Ok(output) => {
                        scan.output = Some(output);
                        scan.failed = false;
                    }
                    Err(error) => {
                        eprintln!("[dcc-native] completed tasks scan: {error}");
                        scan.failed = true;
                    }
                }
                cx.notify();
                // The set may have changed while this pass ran.
                sidebar.refresh_completed_scan(cx);
            });
        })
        .detach();
    }

    /// A pass over a non-empty set of completed tasks is still running.
    pub(super) fn completed_scan_loading(&self) -> bool {
        let scan = &self.completed_scan;
        !scan.failed && scan.output.is_none() && (scan.running || !scan.key.is_empty())
    }

    /// The total on disk, once measured.
    pub(super) fn completed_bytes(&self) -> Option<u64> {
        if self.completed_scan.failed {
            return None;
        }
        self.completed_scan
            .output
            .as_ref()
            .map(|output| output.total_bytes)
    }
}
