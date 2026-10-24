//! The model picker's rules (`model-picker.logic.ts`, `model-favorites.ts`):
//! which providers it offers, the rows of a tab or a search, favorites with
//! their effort, the tab it opens on and the effort slider's stops.

use dcc_core::domain::provider::{HealthStatus, ProviderDescriptor, ProviderModelDescriptor};
use serde::{Deserialize, Serialize};

pub const FAVORITES_TAB: &str = "favorites";
pub const MAX_SHORTCUT_ROWS: usize = 9;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFavorite {
    pub provider_id: String,
    pub model_id: String,
    /// `None`: the model manages effort itself.
    pub effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRow {
    pub key: String,
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub description: String,
    /// Picking a favorite applies its effort too.
    pub favorite: Option<ModelFavorite>,
    /// A search match from a provider other than the open tab.
    pub from_other_provider: bool,
}

pub fn is_enabled(provider: &ProviderDescriptor) -> bool {
    provider.enabled
}

/// `isProviderReady`: switched on and healthy.
pub fn is_ready(provider: &ProviderDescriptor) -> bool {
    is_enabled(provider) && matches!(provider.health, HealthStatus::Healthy)
}

/// `getProviderSetupReason`
pub fn setup_reason(provider: &ProviderDescriptor) -> Option<String> {
    match &provider.health {
        HealthStatus::Healthy => None,
        HealthStatus::Degraded { reason } | HealthStatus::Unhealthy { reason } => {
            Some(reason.clone())
        }
    }
}

/// `pickerProviders`: ready ones, plus the current one while enabled.
pub fn picker_providers<'a>(
    providers: &'a [ProviderDescriptor],
    keep: Option<&str>,
) -> Vec<&'a ProviderDescriptor> {
    providers
        .iter()
        .filter(|provider| {
            is_ready(provider) || (Some(provider.id.0.as_str()) == keep && is_enabled(provider))
        })
        .collect()
}

/// `providersNeedingSetup`
pub fn providers_needing_setup<'a>(
    providers: &'a [ProviderDescriptor],
    keep: Option<&str>,
) -> Vec<&'a ProviderDescriptor> {
    providers
        .iter()
        .filter(|provider| {
            is_enabled(provider) && !is_ready(provider) && Some(provider.id.0.as_str()) != keep
        })
        .collect()
}

/// `pickerModels`: Cursor lists every routed variant; only plain models show.
pub fn picker_models(provider: &ProviderDescriptor) -> Vec<&ProviderModelDescriptor> {
    provider
        .models
        .iter()
        .filter(|model| {
            provider.id.0 != "cursor"
                || model.id.trim().eq_ignore_ascii_case("auto")
                || !model.id.contains(" - ")
        })
        .collect()
}

fn matches(query: &str, haystack: &str) -> bool {
    let target = haystack.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|token| target.contains(token))
}

fn row(provider: &ProviderDescriptor, model: &ProviderModelDescriptor, other: bool) -> PickerRow {
    PickerRow {
        key: format!("{}:{}", provider.id.0, model.id),
        provider_id: provider.id.0.clone(),
        model_id: model.id.clone(),
        label: model.label.clone(),
        description: model.description.clone(),
        favorite: None,
        from_other_provider: other,
    }
}

/// `resolveModelFavorite`: usable while its provider is on and the model
/// still takes that effort.
pub fn favorite_available<'a>(
    favorite: &ModelFavorite,
    providers: &[&'a ProviderDescriptor],
) -> Option<(&'a ProviderDescriptor, &'a ProviderModelDescriptor)> {
    let provider = providers
        .iter()
        .find(|provider| provider.id.0 == favorite.provider_id)?;
    let model = provider
        .models
        .iter()
        .find(|model| model.id == favorite.model_id)?;
    let supported = match &favorite.effort {
        None => model.effort_levels.is_empty(),
        Some(effort) => {
            model.effort_levels.contains(effort)
                || (effort == "ultrathink" && !model.effort_levels.is_empty())
        }
    };
    (is_enabled(provider) && supported).then_some((*provider, model))
}

fn search(providers: &[&ProviderDescriptor], skip: Option<&str>, query: &str) -> Vec<PickerRow> {
    providers
        .iter()
        .filter(|provider| Some(provider.id.0.as_str()) != skip)
        .flat_map(|provider| {
            picker_models(provider)
                .into_iter()
                .filter(|model| {
                    matches(
                        query,
                        &format!("{} {} {}", provider.label, model.label, model.id),
                    )
                })
                .map(|model| row(provider, model, true))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `buildModelPickerRows`
pub fn rows(
    providers: &[ProviderDescriptor],
    favorites: &[ModelFavorite],
    tab: &str,
    query: &str,
    selected_provider: Option<&str>,
) -> Vec<PickerRow> {
    let providers = picker_providers(providers, selected_provider);
    let query = query.trim();
    if tab == FAVORITES_TAB {
        let favorite_rows: Vec<PickerRow> = favorites
            .iter()
            .filter_map(|favorite| {
                let (provider, model) = favorite_available(favorite, &providers)?;
                if !matches(
                    query,
                    &format!("{} {} {}", provider.label, model.label, model.id),
                ) {
                    return None;
                }
                Some(PickerRow {
                    key: serde_json::to_string(&(
                        favorite.provider_id.as_str(),
                        favorite.model_id.as_str(),
                        &favorite.effort,
                    ))
                    .unwrap_or_default(),
                    favorite: Some(favorite.clone()),
                    ..row(provider, model, false)
                })
            })
            .collect();
        if query.is_empty() {
            return favorite_rows;
        }
        let starred: Vec<String> = favorite_rows
            .iter()
            .map(|row| format!("{}:{}", row.provider_id, row.model_id))
            .collect();
        let others = search(&providers, None, query)
            .into_iter()
            .filter(|row| !starred.contains(&row.key));
        return favorite_rows.into_iter().chain(others).collect();
    }
    let active = providers
        .iter()
        .find(|provider| provider.id.0 == tab)
        .copied();
    let own: Vec<PickerRow> = active.map_or_else(Vec::new, |active| {
        picker_models(active)
            .into_iter()
            .filter(|model| matches(query, &format!("{} {}", model.label, model.id)))
            .map(|model| row(active, model, false))
            .collect()
    });
    if query.is_empty() {
        return own;
    }
    let others = search(
        &providers,
        active.map(|provider| provider.id.0.as_str()),
        query,
    );
    own.into_iter().chain(others).collect()
}

/// `initialModelPickerTab`: favorites when one is usable, else the current provider.
pub fn initial_tab(
    providers: &[ProviderDescriptor],
    favorites: &[ModelFavorite],
    selected_provider: Option<&str>,
) -> String {
    let visible = picker_providers(providers, selected_provider);
    if favorites
        .iter()
        .any(|favorite| favorite_available(favorite, &visible).is_some())
    {
        return FAVORITES_TAB.into();
    }
    visible
        .iter()
        .find(|provider| Some(provider.id.0.as_str()) == selected_provider)
        .or(visible.first())
        .map_or_else(|| FAVORITES_TAB.into(), |provider| provider.id.0.clone())
}

/// `effortSliderSteps`: the model's ladder with ultrathink on top.
pub fn effort_steps(levels: &[String]) -> Vec<String> {
    if levels.is_empty() {
        return Vec::new();
    }
    levels
        .iter()
        .cloned()
        .chain(std::iter::once("ultrathink".to_string()))
        .collect()
}

pub fn is_favorited(favorites: &[ModelFavorite], provider: &str, model: &str) -> bool {
    favorites
        .iter()
        .any(|favorite| favorite.provider_id == provider && favorite.model_id == model)
}

/// `parseModelFavorites`: valid entries, each once.
pub fn clean_favorites(favorites: Vec<ModelFavorite>) -> Vec<ModelFavorite> {
    let mut out: Vec<ModelFavorite> = Vec::new();
    for favorite in favorites {
        let valid = !favorite.provider_id.trim().is_empty()
            && !favorite.model_id.trim().is_empty()
            && favorite
                .effort
                .as_deref()
                .is_none_or(|effort| !effort.trim().is_empty());
        if valid && !out.contains(&favorite) {
            out.push(favorite);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, healthy: bool, models: &[(&str, &[&str])]) -> ProviderDescriptor {
        let models: Vec<serde_json::Value> = models
            .iter()
            .map(|(model, levels)| serde_json::json!({ "id": model, "label": model.to_uppercase(), "description": "", "recommended": false, "effortLevels": levels }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "id": id, "label": id, "description": "", "models": models,
            "capabilities": { "streaming": true, "mcpSupport": "unsupported", "tools": true, "vision": true,
                "resumable": true, "experimental": false, "canBeDelegationTarget": true, "canRequestDelegation": true,
                "supportsReadOnlyDelegation": true, "supportsEditDelegation": true },
            "health": if healthy { serde_json::json!("Healthy") } else { serde_json::json!({ "Unhealthy": { "reason": "not signed in" } }) },
            "stable": true
        }))
        .unwrap()
    }

    fn catalog() -> Vec<ProviderDescriptor> {
        vec![
            provider(
                "claude_code",
                true,
                &[("sonnet", &["low", "high"]), ("opus", &["low", "high"])],
            ),
            provider("codex", true, &[("gpt", &["medium"])]),
            provider("cursor", true, &[("auto", &[]), ("gpt - fast", &[])]),
            provider("gemini", false, &[("flash", &[])]),
        ]
    }

    #[test]
    fn offers_ready_providers_and_keeps_the_current_one() {
        let catalog = catalog();
        let ids = |list: Vec<&ProviderDescriptor>| {
            list.iter().map(|p| p.id.0.clone()).collect::<Vec<_>>()
        };
        assert_eq!(
            ids(picker_providers(&catalog, None)),
            ["claude_code", "codex", "cursor"]
        );
        assert!(ids(picker_providers(&catalog, Some("gemini"))).contains(&"gemini".to_string()));
        assert_eq!(ids(providers_needing_setup(&catalog, None)), ["gemini"]);
        assert_eq!(
            picker_models(&catalog[2]).len(),
            1,
            "cursor hides routed variants"
        );
    }

    #[test]
    fn tabs_search_and_favorites_build_the_rows() {
        let catalog = catalog();
        let own = rows(&catalog, &[], "claude_code", "", None);
        assert_eq!(
            own.iter()
                .map(|row| row.model_id.as_str())
                .collect::<Vec<_>>(),
            ["sonnet", "opus"]
        );
        let searched = rows(&catalog, &[], "claude_code", "gpt", None);
        assert_eq!(searched.len(), 1);
        assert!(searched[0].from_other_provider);
        let favorite = ModelFavorite {
            provider_id: "claude_code".into(),
            model_id: "opus".into(),
            effort: Some("high".into()),
        };
        let favorites = vec![favorite.clone()];
        assert_eq!(
            initial_tab(&catalog, &favorites, Some("codex")),
            FAVORITES_TAB
        );
        assert_eq!(initial_tab(&catalog, &[], Some("codex")), "codex");
        let starred = rows(&catalog, &favorites, FAVORITES_TAB, "", None);
        assert_eq!(starred[0].favorite, Some(favorite.clone()));
        let unusable = ModelFavorite {
            effort: Some("max".into()),
            ..favorite
        };
        assert!(rows(&catalog, &[unusable], FAVORITES_TAB, "", None).is_empty());
        assert_eq!(
            effort_steps(&["low".into()]),
            vec!["low".to_string(), "ultrathink".to_string()]
        );
        assert!(effort_steps(&[]).is_empty());
    }
}
