//! What the reviewer runs on in the Pull Requests hub, picked in its panel
//! and kept in `pr-review-pick.json`: the provider, model, effort and
//! account for reviews, apart from the reviewer's own settings, so a review
//! on another account or model never means editing the reviewer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewPick {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `Some("")` is the system login; `None` follows the account picked
    /// for new conversations.
    pub account: Option<String>,
}

impl ReviewPick {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(self).unwrap_or_default())
    }

    /// A model on a provider; the account and effort belong to the old one.
    pub fn pick_model(&mut self, provider_id: &str, model: &str) {
        if self.provider_id.as_deref() != Some(provider_id) {
            self.account = None;
        }
        self.provider_id = Some(provider_id.to_string());
        self.model = Some(model.to_string());
    }

    /// The pick's own choice for this provider, if it made one.
    pub fn on(&self, provider_id: &str) -> Option<&Self> {
        (self.provider_id.as_deref() == Some(provider_id)).then_some(self)
    }
}

fn path() -> PathBuf {
    crate::prefs::native_file("pr-review-pick.json")
}

/// The person's pick (none under test: never the real file).
pub fn load() -> ReviewPick {
    if cfg!(test) {
        return ReviewPick::default();
    }
    ReviewPick::load(&path())
}

pub fn save(pick: &ReviewPick) {
    if !cfg!(test) {
        let _ = pick.save(&path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_survives_and_another_provider_drops_its_account() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pick.json");
        let mut pick = ReviewPick::default();
        pick.pick_model("codex", "gpt-5");
        pick.account = Some("zedy".into());
        pick.effort = Some("high".into());
        pick.save(&file).unwrap();
        let mut read = ReviewPick::load(&file);
        assert_eq!(read, pick);

        read.pick_model("codex", "gpt-5-mini");
        assert_eq!(read.account.as_deref(), Some("zedy"));
        read.pick_model("claude_code", "opus");
        assert_eq!(read.account, None);
        assert!(read.on("codex").is_none());
        assert_eq!(
            ReviewPick::load(&dir.path().join("none.json")),
            ReviewPick::default()
        );
    }
}
