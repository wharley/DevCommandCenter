//! Disk use of completed tasks: the size as the desktop writes it
//! (`formatDiskBytes`) and the cleanup list (`completed-cleanup.ts`).

use std::collections::{HashMap, HashSet};

use dcc_infra::workspace_cleanup::{WorkspaceCleanupEntry, WorkspaceCleanupSafety};

use super::tasks::TaskSummary;
use crate::i18n::Locale;

/// `formatDiskBytes`: binary units, one decimal under 10, the locale's marks.
pub fn format_disk_bytes(bytes: u64, locale: Locale) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = if unit == 0 || value >= 10.0 { 0 } else { 1 };
    let (group, decimal) = match locale {
        Locale::PtBr => ('.', ','),
        Locale::En => (',', '.'),
    };
    let scaled = (value * 10f64.powi(decimals)).round() as u64;
    let (whole, fraction) = if decimals == 0 {
        (scaled, 0)
    } else {
        (scaled / 10, scaled % 10)
    };
    let mut digits = whole.to_string();
    let mut grouped = String::new();
    while digits.len() > 3 {
        let tail = digits.split_off(digits.len() - 3);
        grouped = format!("{group}{tail}{grouped}");
    }
    let mut text = format!("{digits}{grouped}");
    if fraction > 0 {
        text.push(decimal);
        text.push_str(&fraction.to_string());
    }
    format!("{text} {}", UNITS[unit])
}

/// The task ids whose worktrees count for a row: a multi-project task's members.
pub fn disk_usage_ids(tasks: &[&TaskSummary]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for task in tasks {
        let members = if task.member_workspace_ids.is_empty() {
            std::slice::from_ref(&task.id)
        } else {
            task.member_workspace_ids.as_slice()
        };
        for id in members {
            if seen.insert(id.clone()) {
                ids.push(id.clone());
            }
        }
    }
    ids
}

/// One completed task in the cleanup list, members folded in.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanupRow {
    pub task_id: String,
    pub bytes: u64,
    pub safety: WorkspaceCleanupSafety,
    pub changed_files: u32,
    pub unpushed_commits: u32,
}

/// A multi-project task is as risky as its riskiest member.
fn rank(safety: WorkspaceCleanupSafety) -> u8 {
    match safety {
        WorkspaceCleanupSafety::NoWorktree => 0,
        WorkspaceCleanupSafety::Safe => 1,
        WorkspaceCleanupSafety::Unpushed => 2,
        WorkspaceCleanupSafety::Unknown => 3,
        WorkspaceCleanupSafety::Uncommitted => 4,
    }
}

/// `completedCleanupRows`: one row per task, largest first.
pub fn cleanup_rows(tasks: &[&TaskSummary], entries: &[WorkspaceCleanupEntry]) -> Vec<CleanupRow> {
    let by_id: HashMap<&str, &WorkspaceCleanupEntry> = entries
        .iter()
        .map(|entry| (entry.workspace_id.as_str(), entry))
        .collect();
    let mut rows: Vec<CleanupRow> = tasks
        .iter()
        .map(|task| {
            let mut row = CleanupRow {
                task_id: task.id.clone(),
                bytes: 0,
                safety: WorkspaceCleanupSafety::NoWorktree,
                changed_files: 0,
                unpushed_commits: 0,
            };
            for id in disk_usage_ids(&[task]) {
                let Some(member) = by_id.get(id.as_str()) else {
                    row.safety = WorkspaceCleanupSafety::Unknown;
                    continue;
                };
                row.bytes += member.bytes;
                row.changed_files += member.changed_files;
                row.unpushed_commits += member.unpushed_commits;
                if rank(member.safety) > rank(row.safety) {
                    row.safety = member.safety;
                }
            }
            row
        })
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.bytes));
    rows
}

/// Checked when the list opens: safe to delete, and deleting frees something.
pub fn preselected(row: &CleanupRow) -> bool {
    row.safety == WorkspaceCleanupSafety::Safe && row.bytes > 0
}

/// What deleting one task frees, when its members were measured.
pub fn task_bytes(task: &TaskSummary, entries: &[WorkspaceCleanupEntry]) -> Option<u64> {
    let ids = disk_usage_ids(&[task]);
    let measured: Vec<u64> = entries
        .iter()
        .filter(|entry| ids.contains(&entry.workspace_id))
        .map(|entry| entry.bytes)
        .collect();
    (!measured.is_empty()).then(|| measured.iter().sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_desktop() {
        assert_eq!(format_disk_bytes(0, Locale::PtBr), "0 B");
        assert_eq!(format_disk_bytes(1000, Locale::PtBr), "1.000 B");
        assert_eq!(format_disk_bytes(1000, Locale::En), "1,000 B");
        assert_eq!(format_disk_bytes(1536, Locale::PtBr), "1,5 KB");
        assert_eq!(format_disk_bytes(1536, Locale::En), "1.5 KB");
        assert_eq!(format_disk_bytes(1024 * 1024, Locale::En), "1 MB");
        assert_eq!(
            format_disk_bytes(15 * 1024 * 1024 * 1024 + 1, Locale::En),
            "15 GB"
        );
    }

    fn entry(id: &str, bytes: u64, safety: WorkspaceCleanupSafety) -> WorkspaceCleanupEntry {
        WorkspaceCleanupEntry {
            workspace_id: id.into(),
            bytes,
            safety,
            changed_files: 0,
            unpushed_commits: 0,
        }
    }

    #[test]
    fn a_bundle_is_as_risky_as_its_riskiest_member() {
        let mut bundle = TaskSummary::test("b");
        bundle.member_workspace_ids = vec!["m1".into(), "m2".into()];
        let single = TaskSummary::test("s");
        let entries = [
            entry("m1", 10, WorkspaceCleanupSafety::Safe),
            entry("m2", 5, WorkspaceCleanupSafety::Unpushed),
            entry("s", 100, WorkspaceCleanupSafety::Safe),
        ];
        let rows = cleanup_rows(&[&bundle, &single], &entries);
        assert_eq!(rows[0].task_id, "s");
        assert!(preselected(&rows[0]));
        assert_eq!(rows[1].bytes, 15);
        assert_eq!(rows[1].safety, WorkspaceCleanupSafety::Unpushed);
        assert!(!preselected(&rows[1]));
        assert_eq!(task_bytes(&bundle, &entries), Some(15));
        assert_eq!(task_bytes(&TaskSummary::test("x"), &entries), None);
    }
}
