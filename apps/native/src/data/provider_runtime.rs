//! Settings → Providers → "Configuração avançada" (`provider-runtime-settings.ts`,
//! `configured-executable.ts`): the executable, homes and subagent limit DCC
//! launches each provider with. The desktop keeps them in the webview
//! (`dcc.providerRuntimeConfigs`); the native app in `provider-runtime.json`,
//! with the same rules, and sends them with every new conversation and turn.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dcc_core::domain::provider::{Capabilities, HealthStatus, ProviderDescriptor};
use dcc_core::ports::provider::ProviderRuntimeConfig;
use serde::{Deserialize, Serialize};

/// `SUBAGENT_CONCURRENCY_OPTIONS`
pub const SUBAGENT_OPTIONS: [u16; 5] = [1, 2, 4, 6, 8];

/// `ProviderRuntimeDraft`: what the fields hold, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Draft {
    pub binary_path: String,
    pub home_path: String,
    pub shadow_home_path: String,
    /// "" (automatic) or one of the options.
    pub max_concurrent_subagents: String,
}

impl Draft {
    /// Something set: the section opens and says "Personalizado".
    pub fn customized(&self) -> bool {
        !self.binary_path.trim().is_empty()
            || !self.home_path.trim().is_empty()
            || !self.shadow_home_path.trim().is_empty()
            || !self.max_concurrent_subagents.is_empty()
    }
}

/// `normalizeMaxConcurrentSubagents`
fn normalize_subagents(value: &str) -> String {
    if SUBAGENT_OPTIONS
        .iter()
        .any(|option| option.to_string() == value)
    {
        value.to_string()
    } else {
        String::new()
    }
}

/// `supportsProviderRuntime`: the section exists only with one of the fields.
pub fn supports_runtime(capabilities: &Capabilities) -> bool {
    capabilities.supports_runtime_binary
        || capabilities.supports_runtime_home
        || capabilities.supports_shadow_home
        || capabilities.supports_subagent_concurrency
}

/// `draftToProviderRuntimeConfig`: the fields the adapter honors, trimmed;
/// `None` when nothing is left (the backend rejects fields it would ignore).
pub fn to_config(draft: &Draft, capabilities: &Capabilities) -> Option<ProviderRuntimeConfig> {
    let keep = |supported: bool, value: &str| {
        let value = value.trim();
        (supported && !value.is_empty()).then(|| value.to_string())
    };
    let binary_path = keep(capabilities.supports_runtime_binary, &draft.binary_path);
    let home_path = keep(capabilities.supports_runtime_home, &draft.home_path);
    let shadow_home_path = keep(capabilities.supports_shadow_home, &draft.shadow_home_path);
    let max_concurrent_subagents = capabilities
        .supports_subagent_concurrency
        .then(|| normalize_subagents(&draft.max_concurrent_subagents))
        .and_then(|value| value.parse().ok());
    if binary_path.is_none()
        && home_path.is_none()
        && shadow_home_path.is_none()
        && max_concurrent_subagents.is_none()
    {
        return None;
    }
    Some(ProviderRuntimeConfig {
        binary_path,
        home_path,
        shadow_home_path,
        max_concurrent_subagents,
        ..Default::default()
    })
}

/// Every provider's draft, by id (`ProviderRuntimeSettings`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Store(BTreeMap<String, Draft>);

pub fn path() -> PathBuf {
    crate::prefs::native_file("provider-runtime.json")
}

/// The person's drafts (none under test: never the real file).
pub fn load() -> Store {
    if cfg!(test) {
        return Store::default();
    }
    Store::load(&path())
}

/// The runtime a new conversation on this provider starts with (the
/// adapter's capabilities are its built-in ones). Blocking: reads the file.
pub fn for_provider(provider_id: &str) -> Option<ProviderRuntimeConfig> {
    let provider = dcc_providers::offline_catalog()
        .providers
        .into_iter()
        .find(|provider| provider.id.0 == provider_id)?;
    Some(load().runtime_for(&provider))
}

impl Store {
    /// `readProviderRuntimeSettings`: unreadable or unknown values read as none.
    pub fn load(path: &Path) -> Self {
        let mut store: Self = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for draft in store.0.values_mut() {
            draft.max_concurrent_subagents = normalize_subagents(&draft.max_concurrent_subagents);
        }
        store
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec(self).unwrap_or_default())
    }

    /// `getProviderRuntimeDraft`
    pub fn draft(&self, provider_id: &str) -> Draft {
        self.0.get(provider_id).cloned().unwrap_or_default()
    }

    /// `setProviderRuntimeDraft`: an empty draft is removed.
    pub fn set(&mut self, provider_id: &str, draft: Draft) {
        if draft.customized() {
            self.0.insert(provider_id.to_string(), draft);
        } else {
            self.0.remove(provider_id);
        }
    }

    /// `clearProviderRuntimeDraft` ("Restaurar padrão").
    pub fn clear(&mut self, provider_id: &str) {
        self.0.remove(provider_id);
    }

    /// The runtime a turn on this provider carries. Always explicit for a
    /// chosen provider: an empty one tells the backend to drop the
    /// conversation's old runtime, where none would keep it.
    pub fn runtime_for(&self, provider: &ProviderDescriptor) -> ProviderRuntimeConfig {
        to_config(&self.draft(&provider.id.0), &provider.capabilities).unwrap_or_default()
    }

    /// `configuredExecutables` + `withConfiguredExecutableHealth`: a provider
    /// whose configured executable cannot run needs setup whatever its
    /// healthcheck said (Degraded: it leaves the picker, a conversation on it
    /// still says why its turn stopped).
    pub fn apply_executable_health(&self, providers: &mut [ProviderDescriptor]) {
        for provider in providers {
            if !provider.capabilities.supports_runtime_binary {
                continue;
            }
            let draft = self.draft(&provider.id.0);
            let path = draft.binary_path.trim();
            if !path.is_empty() && !usable_executable(Path::new(path)) {
                provider.health = HealthStatus::Degraded {
                    reason: format!(
                        "the configured {} executable path is invalid",
                        provider.label
                    ),
                };
            }
        }
    }
}

/// `check_provider_executable`: an absolute path to an executable file.
pub fn usable_executable(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(binary: bool, home: bool, subagents: bool) -> Capabilities {
        let mut capabilities = dcc_providers::offline_catalog().providers[0]
            .capabilities
            .clone();
        capabilities.supports_runtime_binary = binary;
        capabilities.supports_runtime_home = home;
        capabilities.supports_shadow_home = false;
        capabilities.supports_subagent_concurrency = subagents;
        capabilities
    }

    #[test]
    fn drafts_become_the_runtime_the_adapter_honors() {
        let draft = Draft {
            binary_path: "  /usr/local/bin/codex ".into(),
            home_path: "/tmp/home".into(),
            shadow_home_path: "/tmp/shadow".into(),
            max_concurrent_subagents: "4".into(),
        };
        let config = to_config(&draft, &capabilities(true, false, true)).unwrap();
        assert_eq!(config.binary_path.as_deref(), Some("/usr/local/bin/codex"));
        assert_eq!(config.home_path, None);
        assert_eq!(config.shadow_home_path, None);
        assert_eq!(config.max_concurrent_subagents, Some(4));
        // Nothing the adapter honors: none.
        assert_eq!(to_config(&draft, &capabilities(false, false, false)), None);
        let odd = Draft {
            max_concurrent_subagents: "3".into(),
            ..Draft::default()
        };
        assert_eq!(to_config(&odd, &capabilities(true, true, true)), None);
    }

    #[test]
    fn the_store_keeps_only_what_is_set_and_checks_the_executable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("provider-runtime.json");
        let mut providers = dcc_providers::offline_catalog().providers;
        let binary = providers
            .iter()
            .find(|p| p.capabilities.supports_runtime_binary)
            .map(|p| p.id.0.clone())
            .unwrap();
        let mut store = Store::default();
        store.set(
            &binary,
            Draft {
                binary_path: "/nope/codex".into(),
                ..Draft::default()
            },
        );
        store.set("claude_code", Draft::default());
        store.save(&file).unwrap();
        let store = Store::load(&file);
        assert!(store.draft(&binary).customized());
        assert!(!store.draft("claude_code").customized());
        store.apply_executable_health(&mut providers);
        let provider = providers.iter().find(|p| p.id.0 == binary).unwrap();
        assert!(
            matches!(&provider.health, HealthStatus::Degraded { reason } if reason.contains("executable path is invalid")),
            "{:?}",
            provider.health
        );
        assert!(!usable_executable(Path::new("relative/codex")));
        assert!(usable_executable(Path::new("/bin/sh")));
    }
}
