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
