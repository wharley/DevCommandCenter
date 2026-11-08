//! The suggestions to pass the turn the person acted on in the team room
//! (`room-handoffs.json`, a marker of this app): passed or ignored, by the
//! answer that made it, so its card does not ask again.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Handled {
    Passed,
    Ignored,
}

#[derive(Debug, Default)]
pub struct Handoffs {
    path: Option<PathBuf>,
    handled: BTreeMap<String, (Handled, String)>,
}

impl Handoffs {
    /// `None`: kept in memory only (tests).
    pub fn load(path: Option<PathBuf>) -> Self {
        let handled = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, handled }
    }

    pub fn get(&self, answer: &str) -> Option<Handled> {
        self.handled.get(answer).map(|(handled, _)| *handled)
    }

    pub fn set(&mut self, answer: &str, handled: Handled) {
        let at = chrono::Utc::now().to_rfc3339();
        self.handled.insert(answer.to_string(), (handled, at));
        if self.handled.len() > MAX_ENTRIES {
            let mut entries: Vec<_> = std::mem::take(&mut self.handled).into_iter().collect();
            entries.sort_by(|left, right| right.1.1.cmp(&left.1.1));
            entries.truncate(MAX_ENTRIES);
            self.handled = entries.into_iter().collect();
        }
        if let Some(path) = &self.path
            && let Ok(bytes) = serde_json::to_vec(&self.handled)
        {
            let _ = std::fs::write(path, bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_person_did_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("room-handoffs.json");
        let mut handoffs = Handoffs::load(Some(path.clone()));
        assert_eq!(handoffs.get("s:t"), None);
        handoffs.set("s:t", Handled::Passed);
        handoffs.set("s:u", Handled::Ignored);
        let again = Handoffs::load(Some(path));
        assert_eq!(again.get("s:t"), Some(Handled::Passed));
        assert_eq!(again.get("s:u"), Some(Handled::Ignored));
    }
}
