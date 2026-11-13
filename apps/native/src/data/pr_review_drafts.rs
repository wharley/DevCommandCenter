//! A review being written, kept on disk per pull request: its line
//! comments, its text and its decision survive opening another pull
//! request and restarting the app, until the review is sent. Also which of
//! its files the person marked as seen, each with the patch it had then, so
//! a file that changed asks to be looked at again. Local to this computer:
//! nothing of it is on the forge before the review is sent.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::pr_review::{DraftComment, ReviewEvent};

/// Reviews kept at most; the oldest written goes first.
const MAX_KEPT: usize = 100;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeptDraft {
    pub path: String,
    pub body: String,
    pub line: u32,
    /// `left` or `right`.
    pub side: String,
    pub start_line: Option<u32>,
}

impl From<&DraftComment> for KeptDraft {
    fn from(draft: &DraftComment) -> Self {
        Self {
            path: draft.path.clone(),
            body: draft.body.clone(),
            line: draft.line,
            side: draft.side.to_string(),
            start_line: draft.start_line,
        }
    }
}

impl KeptDraft {
    /// `None` for a side that is neither of a diff's two.
    pub fn draft(&self) -> Option<DraftComment> {
        let side = match self.side.as_str() {
            "left" => "left",
            "right" => "right",
            _ => return None,
        };
        Some(DraftComment {
            path: self.path.clone(),
            body: self.body.clone(),
            line: self.line,
            side,
            start_line: self.start_line,
        })
    }
}

/// What is kept of one pull request's review.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kept {
    pub drafts: Vec<KeptDraft>,
    pub body: String,
    /// `comment`, `approve` or `request_changes`.
    pub event: String,
    /// The files marked as seen: path → the patch's mark when it was.
    pub viewed: BTreeMap<String, String>,
    /// Bumped at each write: the lowest goes when there are too many.
    written: u64,
}

impl Kept {
    pub fn new(drafts: &[DraftComment], body: &str, event: ReviewEvent) -> Self {
        Self {
            drafts: drafts.iter().map(KeptDraft::from).collect(),
            body: body.to_string(),
            event: event.key().to_string(),
            ..Self::default()
        }
    }

    pub fn review_event(&self) -> ReviewEvent {
        ReviewEvent::ALL
            .into_iter()
            .find(|event| event.key() == self.event)
            .unwrap_or_default()
    }

    pub fn review_drafts(&self) -> Vec<DraftComment> {
        self.drafts.iter().filter_map(KeptDraft::draft).collect()
    }

    /// Nothing worth keeping: the entry is dropped.
    fn is_empty(&self) -> bool {
        self.drafts.is_empty()
            && self.body.trim().is_empty()
            && self.viewed.is_empty()
            && self.review_event() == ReviewEvent::Comment
    }
}

/// A patch's mark: it changes when the file's diff does.
pub fn patch_mark(patch: Option<&str>) -> String {
    let patch = patch.unwrap_or_default();
    // FNV-1a: stable across runs, unlike the standard hasher.
    let hash = patch.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{}:{hash:016x}", patch.len())
}

type Store = BTreeMap<String, Kept>;

/// Puts (or, when empty, drops) a pull request's review in the store.
fn put(store: &mut Store, id: &str, mut kept: Kept) {
    if kept.is_empty() {
        store.remove(id);
        return;
    }
    kept.written = store.values().map(|kept| kept.written).max().unwrap_or(0) + 1;
    store.insert(id.to_string(), kept);
    while store.len() > MAX_KEPT {
        let Some(oldest) = store
            .iter()
            .min_by_key(|(_, kept)| kept.written)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        store.remove(&oldest);
    }
}

fn path() -> PathBuf {
    crate::prefs::native_file("pr-review-drafts.json")
}

fn read() -> Store {
    if cfg!(test) {
        return Store::new();
    }
    std::fs::read(path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// What was kept of this pull request's review, if anything.
pub fn load(id: &str) -> Option<Kept> {
    read().remove(id)
}

/// Keeps it (a few KB, written at once: called when a draft is added or
/// removed and when the pull request on screen changes, not while typing).
pub fn save(id: &str, kept: Kept) {
    if cfg!(test) {
        return;
    }
    let mut store = read();
    put(&mut store, id, kept);
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(&store) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(line: u32, side: &'static str) -> DraftComment {
        DraftComment {
            path: "src/a.rs".into(),
            body: "why?".into(),
            line,
            side,
            start_line: Some(line - 1),
        }
    }

    #[test]
    fn a_review_reads_back_as_it_was_written() {
        let drafts = [draft(7, "right"), draft(3, "left")];
        let kept = Kept::new(&drafts, "looks good", ReviewEvent::Approve);
        let json = serde_json::to_vec(&kept).unwrap();
        let back: Kept = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.review_drafts(), drafts);
        assert_eq!(back.review_event(), ReviewEvent::Approve);
        assert_eq!(back.body, "looks good");
        // A side that is no side of a diff is dropped, not guessed.
        let odd = KeptDraft {
            side: "middle".into(),
            ..KeptDraft::default()
        };
        assert_eq!(odd.draft(), None);
    }

    #[test]
    fn an_empty_review_is_dropped_and_the_oldest_goes_when_too_many() {
        let mut store = Store::new();
        put(
            &mut store,
            "a",
            Kept::new(&[draft(2, "right")], "", ReviewEvent::Comment),
        );
        assert_eq!(store.len(), 1);
        // Sent: nothing left to keep.
        put(&mut store, "a", Kept::new(&[], "  ", ReviewEvent::Comment));
        assert!(store.is_empty());
        for index in 0..=MAX_KEPT {
            let kept = Kept::new(&[], "text", ReviewEvent::Comment);
            put(&mut store, &format!("pr-{index}"), kept);
        }
        assert_eq!(store.len(), MAX_KEPT);
        assert!(!store.contains_key("pr-0") && store.contains_key("pr-1"));
    }

    #[test]
    fn a_patchs_mark_changes_with_the_patch() {
        assert_eq!(patch_mark(Some("+a")), patch_mark(Some("+a")));
        assert_ne!(patch_mark(Some("+a")), patch_mark(Some("+b")));
        assert_ne!(patch_mark(None), patch_mark(Some("+a")));
    }
}
