//! The models an installed CLI offers right now, read from the CLI itself
//! (each adapter's `cli_models`: Codex's `model/list`, Claude's control
//! protocol, `grok models`), so a model a CLI update brings shows up without
//! a new DCC. Kept for a few minutes per account; the built-in catalog fills
//! in what a CLI leaves out and stays the list whenever it cannot answer.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dcc_core::domain::provider::ProviderModelDescriptor;
use dcc_core::ports::ProviderRuntimeConfig;
use serde_json::Value;

/// How long a list read from a CLI is trusted.
const FRESH: Duration = Duration::from_secs(5 * 60);
/// How long a CLI that did not answer is left alone.
const RETRY: Duration = Duration::from_secs(60);
/// The longest a read may take.
const DEADLINE: Duration = Duration::from_secs(15);

/// Provider, account, folders and executable: what decides the list.
type Key = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// When a list was read, and what it was (none: the CLI did not answer).
type Entry = (Instant, Option<Vec<ProviderModelDescriptor>>);

static CACHE: Mutex<Option<HashMap<Key, Entry>>> = Mutex::new(None);

fn key(provider_id: &str, runtime: Option<&ProviderRuntimeConfig>) -> Key {
    let runtime = runtime.cloned().unwrap_or_default();
    (
        provider_id.to_string(),
        runtime.account_id,
        runtime.config_dir,
        runtime.home_path,
        runtime.binary_path,
    )
}

/// Whether DCC reads this provider's models from its CLI.
pub fn reads_models(provider_id: &str) -> bool {
    matches!(provider_id, "codex" | "claude_code" | "grok")
}

/// The models the CLI offers for this account, or `None` when it reads none
/// (not one DCC asks, not installed, signed out, too slow).
pub async fn discover(
    provider_id: &str,
    runtime: Option<&ProviderRuntimeConfig>,
) -> Option<Vec<ProviderModelDescriptor>> {
    if !reads_models(provider_id) {
        return None;
    }
    let key = key(provider_id, runtime);
    if let Some(cached) = cached(&key) {
        return cached;
    }
    let read = match crate::provider_runtime(provider_id) {
        Some(adapter) => tokio::time::timeout(DEADLINE, adapter.cli_models(runtime))
            .await
            .ok()
            .and_then(Result::ok)
            .flatten()
            .filter(|models| !models.is_empty()),
        None => None,
    };
    if let Ok(mut cache) = CACHE.lock() {
        cache
            .get_or_insert_with(HashMap::new)
            .insert(key, (Instant::now(), read.clone()));
    }
    read
}

/// Drops every list read from this provider's CLI (it was just updated).
pub fn forget(provider_id: &str) {
    if let Ok(mut cache) = CACHE.lock() {
        if let Some(cache) = cache.as_mut() {
            cache.retain(|key, _| key.0 != provider_id);
        }
    }
}

/// Whether the CLI offers this model to this account (a model DCC's own
/// catalog does not know yet).
pub async fn offers(
    provider_id: &str,
    runtime: Option<&ProviderRuntimeConfig>,
    model: &str,
) -> bool {
    discover(provider_id, runtime)
        .await
        .is_some_and(|models| models.iter().any(|entry| entry.id == model))
}

fn cached(key: &Key) -> Option<Option<Vec<ProviderModelDescriptor>>> {
    let cache = CACHE.lock().ok()?;
    let (at, models) = cache.as_ref()?.get(key)?;
    let trusted = if models.is_some() { FRESH } else { RETRY };
    (at.elapsed() < trusted).then(|| models.clone())
}

/// The effort levels DCC sends; one a CLI adds before DCC does (Codex's
/// "ultra") stays out of the picker until it can.
pub(crate) fn known_effort(level: &str) -> bool {
    matches!(level, "low" | "medium" | "high" | "xhigh" | "max")
}

fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str).map(str::trim)
}

/// Codex names some models "GPT-6.1-Sol" and others "GPT-6.1 Terra": a
/// hyphen between the version and the name reads as a space.
fn codex_label(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(index, &c)| {
            let between = index > 0
                && chars[index - 1].is_ascii_digit()
                && chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_alphabetic());
            if c == '-' && between {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// `model/list` rows: the visible ones, as the picker shows them. The
/// model id is what `-m` takes (`model`, else `id`).
pub(crate) fn codex_models(rows: &[Value]) -> Vec<ProviderModelDescriptor> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|row| row.get("hidden").and_then(Value::as_bool) != Some(true))
        .filter_map(|row| {
            let id = text(row, "model")
                .filter(|id| !id.is_empty())
                .or_else(|| text(row, "id"))
                .filter(|id| !id.is_empty())?;
            if !seen.insert(id.to_string()) {
                return None;
            }
            let effort_levels = row
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|option| {
                    option
                        .get("reasoningEffort")
                        .and_then(Value::as_str)
                        .or_else(|| option.as_str())
                })
                .filter(|level| known_effort(level))
                .map(str::to_string)
                .collect();
            Some(ProviderModelDescriptor {
                id: id.to_string(),
                label: codex_label(
                    text(row, "displayName")
                        .filter(|label| !label.is_empty())
                        .unwrap_or(id),
                ),
                description: text(row, "description").unwrap_or_default().to_string(),
                recommended: row.get("isDefault").and_then(Value::as_bool) == Some(true),
                effort_levels,
            })
        })
        .collect()
}

/// `grok-4.7-build-fast` as "Grok 4.7 Build Fast", for a model the CLI
/// names without a label.
fn label_of(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The provider's models replaced by the ones its CLI offers, when it
/// offered any: in the CLI's order, with the CLI's names. The built-in
/// catalog only fills in what a CLI does not say (a description, the effort
/// levels Grok never lists), and a model the CLI does not name gets one
/// made from its id. Nothing here needs a new DCC for a new model.
pub fn apply(
    models: &mut Vec<ProviderModelDescriptor>,
    discovered: Option<Vec<ProviderModelDescriptor>>,
) {
    let Some(discovered) = discovered.filter(|list| !list.is_empty()) else {
        return;
    };
    let built_in = std::mem::take(models);
    *models = discovered
        .into_iter()
        .map(|mut model| {
            let known = built_in.iter().find(|entry| entry.id == model.id);
            if model.label.is_empty() {
                model.label =
                    known.map_or_else(|| label_of(&model.id), |entry| entry.label.clone());
            }
            if let Some(known) = known {
                if model.description.is_empty() {
                    model.description.clone_from(&known.description);
                }
                if model.effort_levels.is_empty() {
                    model.effort_levels.clone_from(&known.effort_levels);
                }
            }
            model
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_rows_become_the_pickers_models() {
        let rows = [
            json!({ "id": "p1", "model": "gpt-6.1-terra", "displayName": "GPT-6.1 Terra",
                "description": "New", "hidden": false, "isDefault": true,
                "supportedReasoningEfforts": [
                    { "reasoningEffort": "low", "description": "" },
                    { "reasoningEffort": "xhigh", "description": "" },
                    { "reasoningEffort": "ultra", "description": "" }] }),
            json!({ "id": "old", "model": "gpt-5.5", "displayName": "", "hidden": true,
                "supportedReasoningEfforts": [] }),
            json!({ "id": "gpt-6-sol", "displayName": "GPT-6-Sol", "hidden": false,
                "supportedReasoningEfforts": [] }),
            json!({ "id": "p1", "model": "gpt-6.1-terra", "displayName": "again" }),
            json!({ "displayName": "no id" }),
        ];
        let models = codex_models(&rows);
        let ids = |list: &[ProviderModelDescriptor]| {
            list.iter().map(|m| m.id.clone()).collect::<Vec<_>>()
        };
        assert_eq!(ids(&models), ["gpt-6.1-terra", "gpt-6-sol"]);
        assert_eq!(models[0].label, "GPT-6.1 Terra");
        assert!(models[0].recommended);
        assert_eq!(models[0].effort_levels, ["low", "xhigh"]);
        assert_eq!(models[1].label, "GPT-6 Sol");

        let built_in = ProviderModelDescriptor {
            id: "gpt-6-sol".into(),
            label: "GPT-6 Sol".into(),
            description: "Built in".into(),
            recommended: true,
            effort_levels: vec!["low".into(), "max".into()],
        };
        let mut catalog = vec![built_in.clone()];
        apply(&mut catalog, Some(Vec::new()));
        assert_eq!(
            ids(&catalog),
            ["gpt-6-sol"],
            "an empty answer keeps the built-in list"
        );
        let mut unnamed = models.clone();
        unnamed.push(ProviderModelDescriptor {
            id: "grok-4.7-build-fast".into(),
            label: String::new(),
            description: String::new(),
            recommended: false,
            effort_levels: Vec::new(),
        });
        apply(&mut catalog, Some(unnamed));
        assert_eq!(
            ids(&catalog),
            ["gpt-6.1-terra", "gpt-6-sol", "grok-4.7-build-fast"]
        );
        // The CLI's names; what it left out comes from the built-in entry.
        assert_eq!(catalog[0].label, "GPT-6.1 Terra");
        assert_eq!(catalog[1].label, "GPT-6 Sol");
        assert_eq!(catalog[1].description, "Built in");
        assert_eq!(catalog[1].effort_levels, ["low", "max"]);
        assert!(!catalog[1].recommended, "the CLI says which is the default");
        assert_eq!(catalog[2].label, "Grok 4.7 Build Fast");
    }

    #[tokio::test]
    async fn only_providers_dcc_asks_are_read() {
        assert!(!reads_models("gemini"));
        assert!(discover("gemini", None).await.is_none());
        assert!(!offers("gemini", None, "x").await);
    }
}
