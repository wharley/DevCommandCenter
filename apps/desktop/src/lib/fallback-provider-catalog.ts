import type { ProviderCatalog } from "@dcc/contracts";
import { PROVIDER_MODEL_REGISTRY } from "./provider-model-registry";

/**
 * Mirrors `crates/dcc-providers/src` provider descriptors
 * so the UI always has models when `list_providers` is unavailable (e.g. Vite without Tauri).
 * Model lists are sourced from provider-model-registry — update there, not here.
 */
const stableHealth = "Healthy" as const;

const stableCapabilities = {
	streaming: true,
	mcpSupport: "unsupported",
	mcpOauthSupport: "unsupported",
	tools: true,
	vision: true,
	resumable: true,
	experimental: false,
	canBeDelegationTarget: true,
	canRequestDelegation: false,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
	supportsMultiRoot: false,
} as const;

const delegationRequesterCapabilities = {
	...stableCapabilities,
	canRequestDelegation: true,
} as const;

const multiRootDelegationRequesterCapabilities = {
	...delegationRequesterCapabilities,
	supportsMultiRoot: true,
} as const;

const claudeRuntimeMcpCapabilities = {
	...multiRootDelegationRequesterCapabilities,
	mcpOauthSupport: "managedDuringTurn",
	mcpSupport: {
		runtimeBridge: {
			providerVersion: "claude-agent-sdk@0.2.126+claude-code@2.1.126",
		},
	},
} as const;

const codexRuntimeMcpCapabilities = {
	...multiRootDelegationRequesterCapabilities,
	mcpOauthSupport: "interactivePreflight",
	mcpSupport: {
		runtimeBridge: {
			providerVersion: "codex-cli@0.146.0+app-server-protocol-v2",
		},
	},
} as const;

const experimentalCapabilities = {
	streaming: true,
	mcpSupport: "unsupported",
	mcpOauthSupport: "unsupported",
	tools: true,
	vision: false,
	resumable: false,
	experimental: true,
	canBeDelegationTarget: true,
	canRequestDelegation: false,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
	supportsMultiRoot: false,
} as const;

const experimentalMultiRootCapabilities = {
	...experimentalCapabilities,
	supportsMultiRoot: true,
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
			capabilities: { ...claudeRuntimeMcpCapabilities },
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
			capabilities: { ...codexRuntimeMcpCapabilities },
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
			capabilities: { ...stableCapabilities, supportsMultiRoot: true },
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
			capabilities: { ...experimentalMultiRootCapabilities },
			health: stableHealth,
			stable: false,
		},
		{
			id: "grok",
			label: "Grok Build",
			description: "Grok Build provider through the Agent Client Protocol.",
			models: PROVIDER_MODEL_REGISTRY.grok.map((m) => ({
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
	],
};
