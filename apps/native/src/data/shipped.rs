//! The tasks whose merge DCC tells the agent the work started with
//! (`dcc_core::domain::task_shipped`): which were already told, and which
//! merged and wait for their deployment. A small file, so a task is told
//! once across restarts. A task waits at most `WAIT_MINUTES` for a
//! production deployment of its merge commit; then it is told as merged
//! only. The deployment is read by the desktop, from GitHub.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use dcc_core::domain::task_shipped::Outcome;
use dcc_infra::forge::github_shipped::{Shipped, ShippedRead};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::data::desktop_api::{ApiError, DesktopApi};

/// How long a merged task waits for its deployment before it is told as
/// merged only.
pub const WAIT_MINUTES: i64 = 30;
/// Tasks remembered as told.
const MAX_REMEMBERED: usize = 500;

/// What is needed to read a merged task's pull request and say which task
/// it was, kept with the wait: a merged task is completed at once when it
/// is open on screen, and a completed task is no longer a row to read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub name: String,
    /// The project's own checkout: it outlives the task's worktree.
    pub root: String,
    pub number: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Entry {
    /// Merged, an agent started it: its deployment is awaited since then.
    Waiting {
        since: String,
        /// None in a wait kept before this was (the row is read instead).
        #[serde(default)]
        task: Option<Kept>,
    },
    /// Told, or nothing to tell (the person's own task).
    Done { at: String },
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    started_at: String,
    tasks: HashMap<String, Entry>,
}

/// What is remembered of each task, by its id.
#[derive(Default)]
pub struct ShippedStore {
    path: Option<PathBuf>,
    /// When this was first kept: a pull request merged before it is old
    /// news, never told.
    pub started_at: String,
    pub tasks: HashMap<String, Entry>,
}

impl ShippedStore {
    pub fn load(path: Option<PathBuf>) -> Self {
        let stored: Option<Stored> = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok());
        let fresh = stored.is_none();
        let stored = stored.unwrap_or_default();
        let store = Self {
            path,
            started_at: if stored.started_at.is_empty() {
                Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            } else {
                stored.started_at
            },
            tasks: stored.tasks,
        };
        if fresh {
            store.save();
        }
        store
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let stored = Stored {
            started_at: self.started_at.clone(),
            tasks: self.tasks.clone(),
        };
        if let Ok(text) = serde_json::to_string(&stored) {
            let _ = std::fs::write(path, text);
        }
    }

    /// Whether a pull request merged at `merged_at` did so before this was
    /// kept (old news); unknown times count as new.
    pub fn is_old(&self, merged_at: Option<&str>) -> bool {
        let parse = |at: &str| DateTime::parse_from_rfc3339(at).ok();
        match (merged_at.and_then(parse), parse(&self.started_at)) {
            (Some(merged), Some(started)) => merged < started,
            _ => false,
        }
    }

    pub fn set(&mut self, task_id: &str, entry: Entry) {
        self.tasks.insert(task_id.to_string(), entry);
        // The oldest told ones go first.
        if self.tasks.len() > MAX_REMEMBERED {
            let mut done: Vec<(String, String)> = self
                .tasks
                .iter()
                .filter_map(|(id, entry)| match entry {
                    Entry::Done { at } => Some((at.clone(), id.clone())),
                    Entry::Waiting { .. } => None,
                })
                .collect();
            done.sort();
            for (_, id) in done.into_iter().take(self.tasks.len() - MAX_REMEMBERED) {
                self.tasks.remove(&id);
            }
        }
        self.save();
    }

    /// The tasks waiting for their deployment, with since when and what
    /// was kept of each.
    pub fn waiting(&self) -> Vec<(String, String, Option<Kept>)> {
        self.tasks
            .iter()
            .filter_map(|(id, entry)| match entry {
                Entry::Waiting { since, task } => Some((id.clone(), since.clone(), task.clone())),
                Entry::Done { .. } => None,
            })
            .collect()
    }
}

/// What to do with a waiting task after a read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    Wait,
    Tell(Outcome),
    /// Not merged after all (the pull request reopened): forgotten.
    Forget,
}

/// The step after reading where the pull request stands, `since` being
/// when the wait started.
pub fn next(shipped: &Shipped, since: &str, now: DateTime<Utc>) -> Next {
    let waited = DateTime::parse_from_rfc3339(since)
        .map(|since| (now - since.with_timezone(&Utc)).num_minutes())
        .unwrap_or(WAIT_MINUTES);
    match shipped {
        Shipped::Live {
            environment, at, ..
        } => Next::Tell(Outcome::Live {
            environment: environment.clone(),
            at: at.clone(),
        }),
        Shipped::DeployFailed { environment, .. } => Next::Tell(Outcome::DeployFailed {
            environment: environment.clone(),
        }),
        Shipped::Merged | Shipped::Deploying { .. } if waited >= WAIT_MINUTES => {
            Next::Tell(Outcome::Merged)
        }
        Shipped::Merged | Shipped::Deploying { .. } => Next::Wait,
        Shipped::NotMerged if waited >= WAIT_MINUTES => Next::Forget,
        Shipped::NotMerged => Next::Wait,
    }
}

impl DesktopApi {
    /// Where the task's pull request stands on its way out (GitHub).
    pub fn pull_request_shipped(
        &self,
        checkout: &str,
        number: u32,
    ) -> Result<ShippedRead, ApiError> {
        let value = self.request_waiting(
            "POST",
            "/api/v1/git/pr-shipped",
            Some(&json!({ "workspaceRoot": checkout, "number": number })),
            std::time::Duration::from_secs(60),
        )?;
        serde_json::from_value(value).map_err(|error| ApiError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + chrono::Duration::minutes(minutes)
    }

    const SINCE: &str = "2026-11-14T10:00:00Z";

    #[test]
    fn a_merged_task_waits_for_its_deployment_and_then_says_what_it_saw() {
        let live = Shipped::Live {
            environment: "Production".into(),
            at: Some("t".into()),
            url: None,
        };
        assert_eq!(
            next(&live, SINCE, at(1)),
            Next::Tell(Outcome::Live {
                environment: "Production".into(),
                at: Some("t".into())
            })
        );
        let deploying = Shipped::Deploying {
            environment: "Production".into(),
        };
        assert_eq!(next(&deploying, SINCE, at(5)), Next::Wait);
        assert_eq!(next(&Shipped::Merged, SINCE, at(29)), Next::Wait);
        // No deployment seen in time: merged, and no more than that.
        assert_eq!(
            next(&Shipped::Merged, SINCE, at(30)),
            Next::Tell(Outcome::Merged)
        );
        assert_eq!(next(&deploying, SINCE, at(45)), Next::Tell(Outcome::Merged));
        let failed = Shipped::DeployFailed {
            environment: "Production".into(),
            at: None,
        };
        assert_eq!(
            next(&failed, SINCE, at(2)),
            Next::Tell(Outcome::DeployFailed {
                environment: "Production".into()
            })
        );
        assert_eq!(next(&Shipped::NotMerged, SINCE, at(2)), Next::Wait);
        assert_eq!(next(&Shipped::NotMerged, SINCE, at(31)), Next::Forget);
    }

    #[test]
    fn what_was_told_is_remembered_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shipped.json");
        let mut store = ShippedStore::load(Some(path.clone()));
        assert!(store.tasks.is_empty());
        let kept = Kept {
            name: "Corrigir o campo".into(),
            root: "/repo".into(),
            number: 12,
        };
        store.set(
            "t-1",
            Entry::Waiting {
                since: SINCE.into(),
                task: Some(kept.clone()),
            },
        );
        store.set("t-2", Entry::Done { at: SINCE.into() });
        let again = ShippedStore::load(Some(path));
        assert_eq!(
            again.started_at, store.started_at,
            "kept from the first run"
        );
        // What reads its pull request is kept with the wait: the task may
        // be completed, and gone from the rows, by the time it is read.
        assert_eq!(
            again.waiting(),
            [("t-1".to_string(), SINCE.to_string(), Some(kept))]
        );
        // A wait kept before that reads with nothing kept.
        let old: Entry = serde_json::from_str(r#"{"waiting":{"since":"x"}}"#).unwrap();
        assert_eq!(
            old,
            Entry::Waiting {
                since: "x".into(),
                task: None
            }
        );
        assert_eq!(
            again.tasks.get("t-2"),
            Some(&Entry::Done { at: SINCE.into() })
        );
    }

    #[test]
    fn a_merge_from_before_this_was_kept_is_old_news() {
        let store = ShippedStore {
            started_at: "2026-11-14T10:00:00Z".into(),
            ..ShippedStore::default()
        };
        assert!(store.is_old(Some("2026-11-13T19:40:00Z")));
        assert!(!store.is_old(Some("2026-11-14T10:05:00Z")));
        assert!(!store.is_old(None));
    }
}
