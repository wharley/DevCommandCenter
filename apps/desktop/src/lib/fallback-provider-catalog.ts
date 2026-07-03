import type { ProviderCatalog } from "@dcc/contracts";
import { PROVIDER_MODEL_REGISTRY } from "./provider-model-registry";

/**
 * Mirrors `crates/dcc-providers/src/{claude_code,codex,gemini,cursor}.rs` descriptors
 * so the UI always has models when `list_providers` is unavailable (e.g. Vite without Tauri).
 * Model lists are sourced from provider-model-registry — update there, not here.
 */
const stableHealth = "Healthy" as const;

const stableCapabilities = {
	streaming: true,
	mcp: true,
	tools: true,
	vision: true,
	resumable: true,
	experimental: false,
	canBeDelegationTarget: true,
	canRequestDelegation: false,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
} as const;

const delegationRequesterCapabilities = {
	...stableCapabilities,
	canRequestDelegation: true,
} as const;

const experimentalCapabilities = {
	streaming: true,
	mcp: false,
	tools: true,
	vision: false,
	resumable: false,
	experimental: true,
	canBeDelegationTarget: true,
	canRequestDelegation: false,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
} as const;

export const FALLBACK_PROVIDER_CATALOG: ProviderCatalog = {
	providers: [
		{
			id: "claude_code",
			label: "Claude Code",
			description: "Stable Claude CLI provider for agentic coding and tool use.",
			models: PROVIDER_MODEL_REGISTRY.claude_code.map((m) => ({
				id: m.id,
				label: m.label,
				description: m.description,
				recommended: m.recommended,
				effortLevels: m.effortLevels,
			})),
			capabilities: { ...delegationRequesterCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "codex",
			label: "Codex",
			description: "Stable OpenAI Codex provider for repo-aware coding workflows.",
			models: PROVIDER_MODEL_REGISTRY.codex.map((m) => ({
				id: m.id,
				label: m.label,
				description: m.description,
				recommended: m.recommended,
				effortLevels: m.effortLevels,
			})),
			capabilities: { ...delegationRequesterCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "gemini",
			label: "Gemini",
			description: "Stable Gemini CLI provider for workspace tasks.",
			models: PROVIDER_MODEL_REGISTRY.gemini.map((m) => ({
				id: m.id,
				label: m.label,
				description: m.description,
				recommended: m.recommended,
				effortLevels: m.effortLevels,
			})),
			capabilities: { ...stableCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "droid",
			label: "Droid",
			description: "Factory Droid exec provider for workspace coding workflows.",
			models: PROVIDER_MODEL_REGISTRY.droid.map((m) => ({
				id: m.id,
				label: m.label,
				description: m.description,
				recommended: m.recommended,
				effortLevels: m.effortLevels,
			})),
			capabilities: { ...stableCapabilities },
			health: stableHealth,
			stable: true,
		},
		{
			id: "cursor",
			label: "Cursor",
			description: "Cursor Agent CLI provider with CLI-native session resume.",
			models: PROVIDER_MODEL_REGISTRY.cursor.map((m) => ({
				id: m.id,
				label: m.label,
				description: m.description,
				recommended: m.recommended,
				effortLevels: m.effortLevels,
			})),
			capabilities: { ...experimentalCapabilities },
			health: stableHealth,
			stable: false,
		},
	],
};
