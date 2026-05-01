import type { ProviderCatalog } from "@dcc/contracts";

/**
 * Mirrors `crates/dcc-providers/src/{claude_code,codex,gemini,cursor}.rs` descriptors
 * so the UI always has models when `list_providers` is unavailable (e.g. Vite without Tauri).
 */
const stableHealth = "Healthy" as const;

const stableCapabilities = {
	streaming: true,
	mcp: true,
	tools: true,
	vision: true,
	resumable: true,
	experimental: false,
} as const;

const experimentalCapabilities = {
	streaming: true,
	mcp: false,
	tools: true,
	vision: false,
	resumable: false,
	experimental: true,
} as const;

export const FALLBACK_PROVIDER_CATALOG: ProviderCatalog = {
	providers: [
		{
			id: "claude_code",
			label: "Claude Code",
			description: "Stable Claude CLI provider for agentic coding and tool use.",
			models: [
				{
					id: "claude-opus-4-6",
					label: "Claude Opus 4.6",
					description:
						"Highest capability, best for deep reasoning and large refactors.",
					recommended: false,
				},
				{
					id: "claude-sonnet-4-6",
					label: "Claude Sonnet 4.6",
					description: "Balanced default for coding and analysis.",
					recommended: true,
				},
				{
					id: "claude-haiku-4-5",
					label: "Claude Haiku 4.5",
					description: "Fast, lightweight option for quick follow-ups.",
					recommended: false,
				},
			],
			capabilities: { ...stableCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "codex",
			label: "Codex",
			description: "Stable OpenAI Codex provider for repo-aware coding workflows.",
			models: [
				{
					id: "gpt-5-codex",
					label: "GPT-5 Codex",
					description: "Default Codex model for agentic coding.",
					recommended: true,
				},
				{
					id: "gpt-5.3-codex",
					label: "GPT-5.3 Codex",
					description: "Latest Codex variant with stronger reasoning.",
					recommended: false,
				},
			],
			capabilities: { ...stableCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "gemini",
			label: "Gemini",
			description: "Stable Gemini CLI provider for workspace tasks.",
			models: [
				{
					id: "gemini-2.5-pro",
					label: "Gemini 2.5 Pro",
					description: "Balanced default for long-context work.",
					recommended: true,
				},
				{
					id: "gemini-2.5-flash",
					label: "Gemini 2.5 Flash",
					description: "Fast variant for lightweight responses.",
					recommended: false,
				},
			],
			capabilities: { ...stableCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "cursor",
			label: "Cursor",
			description: "Experimental Cursor adapter kept behind the migration boundary.",
			models: [
				{
					id: "cursor-agent",
					label: "Cursor Agent",
					description: "Primary Cursor agent flow.",
					recommended: true,
				},
				{
					id: "cursor-editor",
					label: "Cursor Editor",
					description: "More direct editor-centric workflow.",
					recommended: false,
				},
			],
			capabilities: { ...experimentalCapabilities },
			health: stableHealth,
			stable: false,
		},
	],
};
