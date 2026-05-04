use crate::domain::provider::ProviderModelDescriptor;

pub struct ModelEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub recommended: bool,
}

impl ModelEntry {
    pub fn to_descriptor(&self) -> ProviderModelDescriptor {
        ProviderModelDescriptor {
            id: self.id.to_string(),
            label: self.label.to_string(),
            description: self.description.to_string(),
            recommended: self.recommended,
        }
    }
}

/// Alias tables: (short_alias_or_old_id, canonical_id).
/// When a new model version ships, add the old ID as an alias pointing to the new canonical.
pub const CLAUDE_CODE_ALIASES: &[(&str, &str)] = &[
    ("opus", "claude-opus-4-7"),
    ("opus-4.7", "claude-opus-4-7"),
    ("opus-4.6", "claude-opus-4-6"),
    ("claude-opus-4-6-20251117", "claude-opus-4-6"),
    ("sonnet", "claude-sonnet-4-6"),
    ("sonnet-4.6", "claude-sonnet-4-6"),
    ("claude-sonnet-4-6-20251117", "claude-sonnet-4-6"),
    ("haiku", "claude-haiku-4-5"),
    ("haiku-4.5", "claude-haiku-4-5"),
    ("claude-haiku-4-5-20251001", "claude-haiku-4-5"),
];

pub const CODEX_ALIASES: &[(&str, &str)] = &[
    ("gpt-5-codex", "gpt-5.4"),
    ("5.5", "gpt-5.5"),
    ("5.4", "gpt-5.4"),
    ("5.4-mini", "gpt-5.4-mini"),
    ("5.3", "gpt-5.3-codex"),
    ("gpt-5.3", "gpt-5.3-codex"),
    ("gpt-5.3-spark", "gpt-5.3-codex-spark"),
];

pub const GEMINI_ALIASES: &[(&str, &str)] = &[
    ("pro", "gemini-3.1-pro"),
    ("flash", "gemini-3-flash"),
    ("3.1-pro", "gemini-3.1-pro"),
    ("3-flash", "gemini-3-flash"),
    ("2.5-pro", "gemini-2.5-pro"),
    ("2.5-flash", "gemini-2.5-flash"),
];

/// Resolves a model alias or legacy ID to its canonical form for the given provider.
/// Returns the input unchanged if no alias matches (pass-through for already-canonical IDs).
pub fn resolve_alias(provider_id: &str, model: &str) -> String {
    let aliases: &[(&str, &str)] = match provider_id {
        "claude_code" => CLAUDE_CODE_ALIASES,
        "codex" => CODEX_ALIASES,
        "gemini" => GEMINI_ALIASES,
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
        id: "claude-opus-4-7",
        label: "Claude Opus 4.7",
        description: "Highest capability, best for deep reasoning and large refactors.",
        recommended: false,
    },
    ModelEntry {
        id: "claude-sonnet-4-6",
        label: "Claude Sonnet 4.6",
        description: "Balanced default for coding and analysis.",
        recommended: true,
    },
    ModelEntry {
        id: "claude-haiku-4-5",
        label: "Claude Haiku 4.5",
        description: "Fast, lightweight option for quick follow-ups.",
        recommended: false,
    },
];

pub const CODEX: &[ModelEntry] = &[
    ModelEntry {
        id: "gpt-5.5",
        label: "GPT-5.5",
        description: "Newest Codex model with the strongest reasoning.",
        recommended: false,
    },
    ModelEntry {
        id: "gpt-5.4",
        label: "GPT-5.4",
        description: "Balanced default for agentic coding workflows.",
        recommended: true,
    },
    ModelEntry {
        id: "gpt-5.4-mini",
        label: "GPT-5.4 Mini",
        description: "Fast, lightweight variant for quick tasks.",
        recommended: false,
    },
    ModelEntry {
        id: "gpt-5.3-codex",
        label: "GPT-5.3 Codex",
        description: "Previous-generation Codex with strong repo-aware reasoning.",
        recommended: false,
    },
];

pub const GEMINI: &[ModelEntry] = &[
    ModelEntry {
        id: "gemini-3.1-pro",
        label: "Gemini 3.1 Pro",
        description: "Latest Gemini model with extended context and reasoning.",
        recommended: true,
    },
    ModelEntry {
        id: "gemini-3-flash",
        label: "Gemini 3 Flash",
        description: "Fast Gemini 3 variant for high-throughput tasks.",
        recommended: false,
    },
    ModelEntry {
        id: "gemini-2.5-pro",
        label: "Gemini 2.5 Pro",
        description: "Stable long-context model.",
        recommended: false,
    },
    ModelEntry {
        id: "gemini-2.5-flash",
        label: "Gemini 2.5 Flash",
        description: "Fast stable variant.",
        recommended: false,
    },
];
