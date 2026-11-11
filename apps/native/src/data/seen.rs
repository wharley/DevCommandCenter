//! Which finished results the person already opened (`workspace-seen-results.ts`).
//! A reading marker of this app, not task state: kept in `seen-results.json`.
//! The baseline is stamped on first use so old results don't all show as new.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::time::parse_ms;

const MAX_ENTRIES: usize = 300;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SeenResults {
    pub baseline: Option<String>,
    pub seen: HashMap<String, String>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl SeenResults {
    pub fn load(path: &Path, now: &str) -> Self {
        let mut results: Self = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        results.path = Some(path.to_path_buf());
        if results.baseline.is_none() {
            results.baseline = Some(now.to_string());
            results.save();
        }
        results
    }

    /// `isWorkspaceResultUnread`: finished after the baseline and after the
    /// last result seen for that task. No baseline means nothing is unread.
    pub fn is_unread(&self, workspace_id: &str, completed_at: Option<&str>) -> bool {
        dcc_core::domain::room_inbox::result_unread(
            self.baseline.as_deref(),
            self.seen.get(workspace_id).map(String::as_str),
            completed_at,
        )
    }

    /// Opening a task marks its latest result as seen; returns whether it changed.
    pub fn mark_seen(&mut self, workspace_id: &str, completed_at: &str) -> bool {
        if !self.is_unread(workspace_id, Some(completed_at)) {
            return false;
        }
        self.seen
            .insert(workspace_id.to_string(), completed_at.to_string());
        if self.seen.len() > MAX_ENTRIES {
            let mut entries: Vec<(String, String)> = self.seen.drain().collect();
            entries.sort_by_key(|(_, at)| std::cmp::Reverse(parse_ms(at).unwrap_or(i64::MIN)));
            entries.truncate(MAX_ENTRIES);
            self.seen = entries.into_iter().collect();
        }
        self.save();
        true
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_results_after_the_baseline_and_the_last_seen_are_unread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seen-results.json");
        let mut seen = SeenResults::load(&path, "2026-10-23T10:00:00Z");
        assert!(
            !seen.is_unread("a", Some("2026-10-23T09:00:00Z")),
            "before the baseline"
        );
        assert!(seen.is_unread("a", Some("2026-10-23T11:00:00Z")));
        assert!(seen.mark_seen("a", "2026-10-23T11:00:00Z"));
        assert!(!seen.is_unread("a", Some("2026-10-23T11:00:00Z")));
        assert!(
            seen.is_unread("a", Some("2026-10-23T12:00:00Z")),
            "a newer result"
        );

        let reloaded = SeenResults::load(&path, "2030-01-01T00:00:00Z");
        assert_eq!(reloaded.baseline.as_deref(), Some("2026-10-23T10:00:00Z"));
        assert!(!reloaded.is_unread("a", Some("2026-10-23T11:00:00Z")));
    }
}
