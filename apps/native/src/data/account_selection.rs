//! The account picked per provider for new conversations
//! (`dcc.providerAccountSelection`), kept in `provider-account-selection.json`.
//! Absent means the system login. A conversation stays on the account it
//! started with; the pick only applies to new conversations and to a switch
//! to another provider.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dcc_core::domain::provider::ProviderAccount;
use serde::{Deserialize, Serialize};

/// provider → account id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Selection(BTreeMap<String, String>);

impl Selection {
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

    pub fn get(&self, provider_id: &str) -> Option<&str> {
        self.0.get(provider_id).map(String::as_str)
    }

    /// `selectProviderAccount`: `None` goes back to the system login.
    pub fn set(&mut self, provider_id: &str, account_id: Option<&str>) {
        match account_id.filter(|id| !id.is_empty()) {
            Some(id) => self.0.insert(provider_id.to_string(), id.to_string()),
            None => self.0.remove(provider_id),
        };
    }
}

pub fn path() -> PathBuf {
    crate::prefs::native_file("provider-account-selection.json")
}

/// The person's picks (none under test: never the real file).
pub fn load() -> Selection {
    if cfg!(test) {
        return Selection::default();
    }
    Selection::load(&path())
}

/// Picks one provider's account for new conversations and saves it.
pub fn select(provider_id: &str, account_id: Option<&str>) {
    let mut selection = load();
    selection.set(provider_id, account_id);
    if !cfg!(test) {
        let _ = selection.save(&path());
    }
}

/// The accounts last read from the backend; `None` before the first read.
static KNOWN: Mutex<Option<Vec<ProviderAccount>>> = Mutex::new(None);

/// Keeps the accounts just read, so a pick of a removed one is dropped.
pub fn remember(accounts: &[ProviderAccount]) {
    if let Ok(mut known) = KNOWN.lock() {
        *known = Some(accounts.to_vec());
    }
}

/// The accounts last read, if any were.
pub fn known() -> Option<Vec<ProviderAccount>> {
    KNOWN.lock().ok().and_then(|known| known.clone())
}

/// `resolveTurnAccountId`: the conversation's own account while it stays on
/// its provider, else the pick for new conversations. A pick whose account
/// is gone is the system login. `accounts` is `None` before they were read:
/// the pick is trusted then.
pub fn resolve(
    provider_id: &str,
    conversation: Option<(&str, Option<&str>)>,
    selection: &Selection,
    accounts: Option<&[ProviderAccount]>,
) -> Option<String> {
    if let Some((conversation_provider, account)) = conversation
        && conversation_provider == provider_id
    {
        return account.map(str::to_string);
    }
    let picked = selection.get(provider_id)?;
    let exists = accounts.is_none_or(|accounts| {
        accounts
            .iter()
            .any(|account| account.id == picked && account.provider_id.0 == provider_id)
    });
    exists.then(|| picked.to_string())
}

/// The account a new conversation on this provider starts on.
pub fn for_new_conversation(provider_id: &str) -> Option<String> {
    resolve(provider_id, None, &load(), known().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dcc_core::domain::provider::ProviderId;

    fn account(id: &str, provider: &str) -> ProviderAccount {
        ProviderAccount {
            id: id.into(),
            provider_id: ProviderId(provider.into()),
            label: id.into(),
            config_dir: String::new(),
            managed: true,
            created_at: String::new(),
        }
    }

    #[test]
    fn a_conversation_keeps_its_account_on_its_provider() {
        let mut selection = Selection::default();
        selection.set("codex", Some("codex-work"));
        let accounts = [account("codex-work", "codex")];
        assert_eq!(
            resolve("codex", Some(("codex", None)), &selection, Some(&accounts)),
            None
        );
        assert_eq!(
            resolve(
                "codex",
                Some(("claude_code", Some("x"))),
                &selection,
                Some(&accounts)
            ),
            Some("codex-work".into())
        );
    }

    #[test]
    fn a_removed_or_foreign_pick_is_the_system_login() {
        let mut selection = Selection::default();
        selection.set("codex", Some("gone"));
        assert_eq!(resolve("codex", None, &selection, Some(&[])), None);
        assert_eq!(
            resolve("codex", None, &selection, None),
            Some("gone".into())
        );
        selection.set("codex", Some("claude-work"));
        let accounts = [account("claude-work", "claude_code")];
        assert_eq!(resolve("codex", None, &selection, Some(&accounts)), None);
        selection.set("codex", None);
        assert_eq!(selection.get("codex"), None);
    }
}
