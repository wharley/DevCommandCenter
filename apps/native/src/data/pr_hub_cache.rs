//! The hub's last list, kept on disk: Pull Requests opens on it at once and
//! reads the forges behind it (one slow repository held every row back), then
//! shows what came. Only open pull requests' public fields; none under test.

use std::path::PathBuf;

use super::pr_hub::List;

fn path() -> PathBuf {
    crate::prefs::native_file("pr-hub-list.json")
}

/// The list as it was last read, if any.
pub fn load() -> Option<List> {
    if cfg!(test) {
        return None;
    }
    let bytes = std::fs::read(path()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Keeps `list` for the next opening (blocking: call it off the UI thread).
pub fn save(list: &List) {
    if cfg!(test) {
        return;
    }
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(list) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::super::pr_hub::{Item, List};

    #[test]
    fn a_list_reads_back_as_it_was_kept() {
        let list = List {
            items: vec![Item {
                id: "1".into(),
                number: 7,
                title: "feat".into(),
                review_decision: Some("approved".into()),
                ..Item::default()
            }],
            warnings: Vec::new(),
        };
        let json = serde_json::to_vec(&list).unwrap();
        let back: List = serde_json::from_slice(&json).unwrap();
        assert_eq!(back, list);
    }
}
