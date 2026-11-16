//! The Reviewer's pull request reviews at work, written down while they
//! run. A review lives in memory (`pr_review_jobs`), and what started it
//! is marked as handled the moment it starts: an app that quits in the
//! middle — an update, a restart — dropped the review with nothing left to
//! start it again. Each one is kept here from its start to its end; what
//! is still here when the app opens was cut short and is started again,
//! once (`shell::review_resume`). Whoever is about to restart the app can
//! read the file to see a review is running (`pr-reviews-running.json`).

use std::path::PathBuf;

use dcc_core::domain::agent_trigger::Trigger;
use dcc_core::domain::pr_review_request::PrReviewRequest;
use serde::{Deserialize, Serialize};

/// What started a review, enough to start it again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Started {
    /// The Reviewer's trigger, on a pull request of the person's search.
    #[serde(rename_all = "camelCase")]
    Trigger {
        trigger: Trigger,
        url: String,
        number: u32,
        repository: String,
        title: String,
        author: Option<String>,
    },
    /// An agent asked for it from its conversation.
    Asked { request: PrReviewRequest },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningReview {
    /// The hub's id of the pull request, as its job has it.
    pub pr_id: String,
    pub started_at: String,
    /// Started again after being cut short: not a second time.
    #[serde(default)]
    pub resumed: bool,
    pub started: Started,
}

fn path() -> Option<PathBuf> {
    // A test never writes the person's file.
    (!cfg!(test)).then(|| crate::prefs::native_file("pr-reviews-running.json"))
}

fn read(path: &PathBuf) -> Vec<RunningReview> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write(path: &PathBuf, list: &[RunningReview]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(raw) = serde_json::to_string(list) {
        let _ = std::fs::write(path, raw);
    }
}

/// The reviews written down as running.
pub fn load() -> Vec<RunningReview> {
    path().map(|path| read(&path)).unwrap_or_default()
}

/// A review started.
pub fn add(review: RunningReview) {
    let Some(path) = path() else {
        return;
    };
    let mut list = read(&path);
    list.retain(|known| known.pr_id != review.pr_id);
    list.push(review);
    write(&path, &list);
}

/// A review ended, however it did.
pub fn remove(pr_id: &str) {
    let Some(path) = path() else {
        return;
    };
    let list = read(&path);
    if list.iter().any(|known| known.pr_id == pr_id) {
        let kept: Vec<RunningReview> = list
            .into_iter()
            .filter(|known| known.pr_id != pr_id)
            .collect();
        write(&path, &kept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_review_reads_back_as_it_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pr-reviews-running.json");
        let review = RunningReview {
            pr_id: "github:github.com:/w:1093".into(),
            started_at: "2026-11-16T20:12:19Z".into(),
            resumed: false,
            started: Started::Trigger {
                trigger: Trigger::ReviewRequested,
                url: "https://github.com/acme/app/pull/1093".into(),
                number: 1093,
                repository: "acme/app".into(),
                title: "fix/salles".into(),
                author: Some("daniel".into()),
            },
        };
        write(&path, std::slice::from_ref(&review));
        assert_eq!(read(&path), [review]);
        // Nothing there, or something else: none running.
        assert!(read(&dir.path().join("missing.json")).is_empty());
        std::fs::write(&path, "not json").unwrap();
        assert!(read(&path).is_empty());
    }
}
