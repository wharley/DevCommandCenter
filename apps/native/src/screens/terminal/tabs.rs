//! The dock's tabs by scope (`terminal-tabs-store.ts`): at most 8, named
//! "Terminal N" by the first number free, the active one kept, all saved
//! in `terminal-tabs.json` with the desktop's `dcc-terminal-tabs-v1`
//! shape. Each tab owns a shell: `terminal:<scope>:<tab>`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A soft guard so a scope does not pile up shells.
pub const MAX_TABS: usize = 8;
const FILE_NAME: &str = "terminal-tabs.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeTabs {
    pub tabs: Vec<Tab>,
    #[serde(default)]
    pub active_id: Option<String>,
}

impl ScopeTabs {
    pub fn active(&self) -> Option<&Tab> {
        self.active_id
            .as_deref()
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .or_else(|| self.tabs.first())
    }
}

/// `getTerminalRuntimeId` with the backend's `terminal:` owner prefix.
pub fn owner_key(scope: &str, tab: &str) -> String {
    format!("terminal:{scope}:{tab}")
}

fn next_title(tabs: &[Tab]) -> String {
    (1..=MAX_TABS + 1)
        .map(|n| format!("Terminal {n}"))
        .find(|title| tabs.iter().all(|tab| &tab.title != title))
        .unwrap_or_else(|| format!("Terminal {}", tabs.len() + 1))
}

fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    format!(
        "term-{millis:x}-{:x}{:04x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabStore {
    scopes: BTreeMap<String, ScopeTabs>,
    path: Option<PathBuf>,
}

impl TabStore {
    pub fn default_path() -> PathBuf {
        crate::prefs::native_file(FILE_NAME)
    }

    /// Reads the file; anything unreadable starts empty.
    pub fn load(path: &Path) -> Self {
        let scopes: BTreeMap<String, ScopeTabs> = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let scopes = scopes
            .into_iter()
            .map(|(scope, mut value)| {
                value.tabs.retain(|tab| !tab.id.is_empty());
                if value.active_id.is_none() {
                    value.active_id = value.tabs.first().map(|tab| tab.id.clone());
                }
                (scope, value)
            })
            .collect();
        Self {
            scopes,
            path: Some(path.to_path_buf()),
        }
    }

    pub fn scope(&self, scope: &str) -> ScopeTabs {
        self.scopes.get(scope).cloned().unwrap_or_default()
    }

    fn write(&mut self, scope: &str, next: ScopeTabs) {
        self.scopes.insert(scope.to_string(), next);
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let plain: BTreeMap<_, _> = self
            .scopes
            .iter()
            .filter(|(_, value)| !value.tabs.is_empty())
            .collect();
        let result = serde_json::to_vec_pretty(&plain)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let temporary = path.with_extension("json.tmp");
                fs::write(&temporary, json)?;
                fs::rename(temporary, path)
            });
        if let Err(error) = result {
            eprintln!("[dcc-native] could not save the terminal tabs: {error}");
        }
    }

    /// A new tab, made active; at the cap the last one is focused instead.
    pub fn add(&mut self, scope: &str) -> Option<String> {
        let mut current = self.scope(scope);
        if current.tabs.len() >= MAX_TABS {
            let last = current.tabs.last()?.id.clone();
            if current.active_id.as_deref() != Some(last.as_str()) {
                current.active_id = Some(last.clone());
                self.write(scope, current);
            }
            return Some(last);
        }
        let tab = Tab {
            id: new_id(),
            title: next_title(&current.tabs),
        };
        let id = tab.id.clone();
        current.tabs.push(tab);
        current.active_id = Some(id.clone());
        self.write(scope, current);
        Some(id)
    }

    /// At least one tab; returns the active one.
    pub fn ensure(&mut self, scope: &str) -> Option<String> {
        let current = self.scope(scope);
        match current.active() {
            None => self.add(scope),
            Some(tab) if current.active_id.as_deref() == Some(tab.id.as_str()) => {
                Some(tab.id.clone())
            }
            Some(tab) => {
                let id = tab.id.clone();
                self.write(
                    scope,
                    ScopeTabs {
                        active_id: Some(id.clone()),
                        ..current
                    },
                );
                Some(id)
            }
        }
    }

    /// The neighbour to the right (else the left) takes over an active tab.
    /// Returns whether the tab was there.
    pub fn remove(&mut self, scope: &str, id: &str) -> bool {
        let current = self.scope(scope);
        let Some(index) = current.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        let tabs: Vec<Tab> = current
            .tabs
            .into_iter()
            .filter(|tab| tab.id != id)
            .collect();
        let active_id = if current.active_id.as_deref() == Some(id) {
            tabs.get(index)
                .or_else(|| index.checked_sub(1).and_then(|left| tabs.get(left)))
                .or_else(|| tabs.first())
                .map(|tab| tab.id.clone())
        } else {
            current.active_id
        };
        self.write(scope, ScopeTabs { tabs, active_id });
        true
    }

    /// A tab of a known id, for tests that route its owner key.
    #[cfg(test)]
    pub fn with_tab(mut self, scope: &str, id: &str) -> Self {
        let mut current = self.scope(scope);
        current.tabs.push(Tab {
            id: id.to_string(),
            title: next_title(&current.tabs),
        });
        current.active_id = Some(id.to_string());
        self.scopes.insert(scope.to_string(), current);
        self
    }

    pub fn set_active(&mut self, scope: &str, id: &str) {
        let current = self.scope(scope);
        if current.active_id.as_deref() == Some(id) || current.tabs.iter().all(|tab| tab.id != id) {
            return;
        }
        self.write(
            scope,
            ScopeTabs {
                active_id: Some(id.to_string()),
                ..current
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_are_numbered_by_the_first_free_title() {
        let mut store = TabStore::default();
        let first = store.add("s").unwrap();
        let second = store.add("s").unwrap();
        assert_ne!(first, second);
        assert!(store.remove("s", &first));
        store.add("s");
        let titles: Vec<_> = store
            .scope("s")
            .tabs
            .iter()
            .map(|tab| tab.title.clone())
            .collect();
        assert_eq!(titles, ["Terminal 2", "Terminal 1"]);
    }

    #[test]
    fn the_cap_focuses_the_last_tab() {
        let mut store = TabStore::default();
        for _ in 0..MAX_TABS {
            store.add("s");
        }
        let last = store.scope("s").tabs.last().unwrap().id.clone();
        let first = store.scope("s").tabs[0].id.clone();
        store.set_active("s", &first);
        assert_eq!(store.add("s"), Some(last.clone()));
        assert_eq!(store.scope("s").tabs.len(), MAX_TABS);
        assert_eq!(store.scope("s").active_id, Some(last));
    }

    #[test]
    fn closing_the_active_tab_hands_it_to_a_neighbour() {
        let mut store = TabStore::default();
        let a = store.add("s").unwrap();
        let b = store.add("s").unwrap();
        let c = store.add("s").unwrap();
        store.set_active("s", &b);
        store.remove("s", &b);
        assert_eq!(store.scope("s").active_id, Some(c.clone()));
        store.remove("s", &c);
        assert_eq!(store.scope("s").active_id, Some(a.clone()));
        store.remove("s", &a);
        assert_eq!(store.scope("s").active_id, None);
        assert!(!store.remove("s", "gone"));
    }

    #[test]
    fn tabs_are_kept_across_launches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let mut store = TabStore::load(&path);
        assert_eq!(store.ensure("worktree:w"), store.ensure("worktree:w"));
        store.add("worktree:w");
        let again = TabStore::load(&path);
        assert_eq!(again.scope("worktree:w"), store.scope("worktree:w"));
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(raw["worktree:w"]["activeId"].is_string());
        assert_eq!(owner_key("worktree:w", "t1"), "terminal:worktree:w:t1");
    }
}
