use crate::domain::provider::ProviderModelDescriptor;

pub struct ModelEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub recommended: bool,
    /// Ordered effort levels this model supports. Frontend uses this to populate
    /// the effort picker and clamp the selection when the user switches models.
    pub effort_levels: &'static [&'static str],
}

impl ModelEntry {
    pub fn to_descriptor(&self) -> ProviderModelDescriptor {
        ProviderModelDescriptor {
            id: self.id.to_string(),
            label: self.label.to_string(),
            description: self.description.to_string(),
            recommended: self.recommended,
            effort_levels: self.effort_levels.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Static catalog for one provider, or `None` when the provider discovers its
/// models at runtime and the registry is not authoritative.
pub fn entries_for(provider_id: &str) -> Option<&'static [ModelEntry]> {
    match provider_id {
        "claude_code" => Some(CLAUDE_CODE),
        "codex" => Some(CODEX),
        "gemini" => Some(GEMINI),
        "droid" => Some(DROID),
        "grok" => Some(GROK),
        _ => None,
    }
}

/// True when `model` (or one of its aliases) is in the provider's static
/// catalog. `None` when the provider has no static catalog.
pub fn is_known_model(provider_id: &str, model: &str) -> Option<bool> {
    let entries = entries_for(provider_id)?;
    let canonical = resolve_alias(provider_id, model);
    Some(
        entries
            .iter()
            .any(|entry| entry.id == canonical || entry.id == model),
    )
}

/// Alias tables: (short_alias_or_old_id, canonical_id).
/// When a new model version ships, add the old ID as an alias pointing to the new canonical.
pub const CLAUDE_CODE_ALIASES: &[(&str, &str)] = &[
    ("fable", "claude-fable-5-1"),
    ("fable-5.1", "claude-fable-5-1"),
    ("fable-5-1", "claude-fable-5-1"),
    ("fable-5", "claude-fable-5-1"),
    ("claude-fable-5", "claude-fable-5-1"),
    ("opus", "claude-opus-5"),
    ("opus-5", "claude-opus-5"),
    ("opus-4.8", "claude-opus-5"),
    ("claude-opus-4-8", "claude-opus-5"),
    ("opus-4.7", "claude-opus-5"),
    ("claude-opus-4-7", "claude-opus-5"),
    ("opus-4.6", "claude-opus-5"),
    ("claude-opus-4-6", "claude-opus-5"),
    ("claude-opus-4-6-20251117", "claude-opus-5"),
    ("sonnet", "claude-sonnet-5"),
    ("sonnet-5", "claude-sonnet-5"),
    ("sonnet-4.6", "claude-sonnet-5"),
    ("claude-sonnet-4-6", "claude-sonnet-5"),
    ("claude-sonnet-4-6-20251117", "claude-sonnet-5"),
    ("haiku", "claude-haiku-4-5"),
    ("haiku-4.5", "claude-haiku-4-5"),
    ("claude-haiku-4-5-20251001", "claude-haiku-4-5"),
];

pub const CODEX_ALIASES: &[(&str, &str)] = &[
    ("gpt-5-codex", "gpt-5.4"),
    ("sol", "gpt-5.6-sol"),
    ("5.6-sol", "gpt-5.6-sol"),
    ("terra", "gpt-5.6-terra"),
    ("5.6-terra", "gpt-5.6-terra"),
    ("luna", "gpt-5.6-luna"),
    ("5.6-luna", "gpt-5.6-luna"),
    ("5.5", "gpt-5.5"),
    ("5.4", "gpt-5.4"),
    ("5.4-mini", "gpt-5.4-mini"),
    ("5.3", "gpt-5.3-codex"),
    ("gpt-5.3", "gpt-5.3-codex"),
    ("gpt-5.3-spark", "gpt-5.3-codex-spark"),
];

pub const GEMINI_ALIASES: &[(&str, &str)] = &[
    ("pro", "gemini-2.5-pro"),
    ("flash", "gemini-3.8-flash"),
    ("3.8-flash", "gemini-3.8-flash"),
    ("gemini-3.8-flash", "gemini-3.8-flash"),
    ("3-flash-preview", "gemini-3.8-flash"),
    ("gemini-3-flash-preview", "gemini-3.8-flash"),
    ("3.1-pro", "gemini-2.5-pro"),
    ("3-flash", "gemini-3.8-flash"),
    ("gemini-3.1-pro", "gemini-2.5-pro"),
    ("gemini-3-flash", "gemini-3.8-flash"),
    ("2.5-pro", "gemini-2.5-pro"),
    ("2.5-flash", "gemini-3.8-flash"),
    ("gemini-2.5-flash", "gemini-3.8-flash"),
];

pub const DROID_ALIASES: &[(&str, &str)] = &[
    ("auto", "auto"),
    ("sonnet", "claude-sonnet-5"),
    ("sonnet-5", "claude-sonnet-5"),
    ("claude-sonnet-5", "claude-sonnet-5"),
    ("sonnet-4.6", "claude-sonnet-5"),
    ("claude-sonnet-4-6", "claude-sonnet-5"),
    ("gpt-5.4", "gpt-5.4"),
    ("5.4", "gpt-5.4"),
    ("gpt-5.5", "gpt-5.5"),
    ("5.5", "gpt-5.5"),
    ("gemini-3-flash-preview", "gemini-3-flash-preview"),
];

pub const GROK_ALIASES: &[(&str, &str)] = &[
    ("grok", "grok-4.5"),
    ("4.5", "grok-4.5"),
    ("grok-4-5", "grok-4.5"),
];

/// Resolves a model alias or legacy ID to its canonical form for the given provider.
/// Returns the input unchanged if no alias matches (pass-through for already-canonical IDs).
pub fn resolve_alias(provider_id: &str, model: &str) -> String {
    let aliases: &[(&str, &str)] = match provider_id {
        "claude_code" => CLAUDE_CODE_ALIASES,
        "codex" => CODEX_ALIASES,
        "gemini" => GEMINI_ALIASES,
        "droid" => DROID_ALIASES,
        "grok" => GROK_ALIASES,
        _ => return model.to_string(),
    };
    aliases
        .iter()
        .find(|(alias, _)| *alias == model)
        .map(|(_, canonical)| canonical.to_string())
        .unwrap_or_else(|| model.to_string())
}

pub const CLAUDE_CODE: &[ModelEntry] = &[
    ModelEntry {
        id: "claude-fable-5-1",
        label: "Claude Fable 5.1",
        description:
            "Most capable Claude model for demanding reasoning and long-horizon agentic work.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "claude-opus-5",
        label: "Claude Opus 5",
        description: "Complex agentic coding and enterprise work with a 1M-token context window.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "claude-sonnet-5",
        label: "Claude Sonnet 5",
        description: "Best balance of speed and intelligence for coding and analysis.",
        recommended: true,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "claude-haiku-4-5",
        label: "Claude Haiku 4.5",
        description: "Fast, lightweight option for quick follow-ups.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
];

pub const CODEX: &[ModelEntry] = &[
    ModelEntry {
        id: "gpt-5.6-sol",
        label: "GPT-5.6 Sol",
        description: "Flagship GPT-5.6 model for the most demanding coding and reasoning work. Preview access required.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "gpt-5.6-terra",
        label: "GPT-5.6 Terra",
        description: "Strong lower-cost GPT-5.6 option for coding and reasoning. Preview access required.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "gpt-5.6-luna",
        label: "GPT-5.6 Luna",
        description: "Fastest and most cost-efficient GPT-5.6 option. Preview access required.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "gpt-5.5",
        label: "GPT-5.5",
        description: "Newest Codex model with the strongest reasoning.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "gpt-5.4",
        label: "GPT-5.4",
        description: "Balanced default for agentic coding workflows.",
        recommended: true,
        effort_levels: &["low", "medium", "high", "xhigh", "max"],
    },
    ModelEntry {
        id: "gpt-5.4-mini",
        label: "GPT-5.4 Mini",
        description: "Fast, lightweight variant for quick tasks.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "gpt-5.3-codex",
        label: "GPT-5.3 Codex",
        description: "Previous-generation Codex with strong repo-aware reasoning.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh"],
    },
];

pub const GEMINI: &[ModelEntry] = &[
    ModelEntry {
        id: "gemini-3.8-flash",
        label: "Gemini 3.8 Flash",
        description: "Latest stable model for long-horizon coding and agentic workflows. CLI availability depends on the account rollout.",
        recommended: true,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro",
        description: "Stable long-context model with the broadest CLI compatibility.",
        recommended: false,
        effort_levels: &["low", "medium", "high", "xhigh"],
    },
];

pub const DROID: &[ModelEntry] = &[
    ModelEntry {
        id: "auto",
        label: "Auto",
        description: "Use Droid's default model selection for this account.",
        recommended: true,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "claude-sonnet-5",
        label: "Claude Sonnet 5",
        description: "Balanced Claude option with strong coding capability through Droid.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "gpt-5.4",
        label: "GPT-5.4",
        description: "High capability coding model routed through Droid.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "gpt-5.5",
        label: "GPT-5.5",
        description: "Latest high-reasoning OpenAI option exposed by Droid.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
    ModelEntry {
        id: "gemini-3-flash-preview",
        label: "Gemini 3 Flash Preview",
        description: "Fast Gemini option when available in the local Droid account.",
        recommended: false,
        effort_levels: &["low", "medium", "high"],
    },
];

pub const GROK: &[ModelEntry] = &[ModelEntry {
    id: "grok-4.5",
    label: "Grok 4.5",
    description: "Grok Build coding and agentic model through the local ACP CLI.",
    recommended: true,
    effort_levels: &["low", "medium", "high"],
}];

#[cfg(test)]
mod tests {
    use super::{resolve_alias, CLAUDE_CODE, CODEX, GEMINI, GROK};

    #[test]
    fn codex_registers_gpt_56_preview_models() {
        for id in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
            assert!(CODEX.iter().any(|model| model.id == id));
        }
        assert_eq!(resolve_alias("codex", "sol"), "gpt-5.6-sol");
        assert_eq!(resolve_alias("codex", "5.6-terra"), "gpt-5.6-terra");
        assert_eq!(resolve_alias("codex", "luna"), "gpt-5.6-luna");
    }

    #[test]
    fn gemini_catalog_promotes_38_flash_and_migrates_removed_flash_models() {
        assert_eq!(GEMINI.len(), 2);
        assert_eq!(GEMINI[0].id, "gemini-3.8-flash");
        assert!(GEMINI[0].recommended);
        assert_eq!(GEMINI[1].id, "gemini-2.5-pro");
        assert_eq!(resolve_alias("gemini", "flash"), "gemini-3.8-flash");
        assert_eq!(
            resolve_alias("gemini", "gemini-2.5-flash"),
            "gemini-3.8-flash"
        );
        assert_eq!(
            resolve_alias("gemini", "gemini-3-flash-preview"),
            "gemini-3.8-flash"
        );
    }

    #[test]
    fn grok_aliases_resolve_to_grok_45() {
        assert_eq!(resolve_alias("grok", "grok"), "grok-4.5");
        assert_eq!(resolve_alias("grok", "4.5"), "grok-4.5");
        assert!(GROK.iter().any(|model| model.id == "grok-4.5"));
    }

    #[test]
    fn claude_code_aliases_resolve_fable_51() {
        for alias in ["fable", "fable-5.1", "fable-5", "claude-fable-5"] {
            assert_eq!(resolve_alias("claude_code", alias), "claude-fable-5-1");
        }
        assert!(CLAUDE_CODE
            .iter()
            .any(|model| model.id == "claude-fable-5-1"));
        assert!(!CLAUDE_CODE.iter().any(|model| model.id == "claude-fable-5"));
    }

    #[test]
    fn claude_code_aliases_upgrade_opus_versions_to_opus_5() {
        assert_eq!(resolve_alias("claude_code", "opus"), "claude-opus-5");
        assert_eq!(resolve_alias("claude_code", "opus-5"), "claude-opus-5");
        assert_eq!(resolve_alias("claude_code", "opus-4.8"), "claude-opus-5");
        assert_eq!(
            resolve_alias("claude_code", "claude-opus-4-7"),
            "claude-opus-5"
        );
        assert!(CLAUDE_CODE.iter().any(|model| model.id == "claude-opus-5"));
        assert!(!CLAUDE_CODE
            .iter()
            .any(|model| model.id == "claude-opus-4-8"));
    }

    #[test]
    fn claude_code_aliases_upgrade_sonnet_to_sonnet_5() {
        assert_eq!(resolve_alias("claude_code", "sonnet"), "claude-sonnet-5");
        assert_eq!(resolve_alias("claude_code", "sonnet-5"), "claude-sonnet-5");
        assert_eq!(
            resolve_alias("claude_code", "sonnet-4.6"),
            "claude-sonnet-5"
        );
        assert_eq!(
            resolve_alias("claude_code", "claude-sonnet-4-6"),
            "claude-sonnet-5"
        );
    }
}
