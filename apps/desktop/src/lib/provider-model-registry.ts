export type ModelEntry = {
	id: string;
	label: string;
	description: string;
	recommended: boolean;
	effortLevels: string[];
};

/**
 * Single source of truth for provider model lists.
 * Keys match the Rust provider IDs (e.g. "claude_code").
 * To add a new model version, update only this file.
 */
export const PROVIDER_MODEL_REGISTRY = {
	claude_code: [
		{
			id: "claude-fable-5",
			label: "Claude Fable 5",
			description:
				"Most capable widely available Claude model for demanding reasoning and long-horizon agentic work.",
			recommended: false,
			effortLevels: ["low", "medium", "high", "xhigh", "max"],
		},
		{
			id: "claude-opus-4-8",
			label: "Claude Opus 4.8",
			description: "Highest capability, best for deep reasoning and large refactors.",
			recommended: false,
			effortLevels: ["low", "medium", "high", "xhigh", "max"],
		},
		{
			id: "claude-sonnet-4-6",
			label: "Claude Sonnet 4.6",
			description: "Balanced default for coding and analysis.",
			recommended: true,
			effortLevels: ["low", "medium", "high", "xhigh"],
		},
		{
			id: "claude-haiku-4-5",
			label: "Claude Haiku 4.5",
			description: "Fast, lightweight option for quick follow-ups.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
	] satisfies ModelEntry[],

	codex: [
		{
			id: "gpt-5.5",
			label: "GPT-5.5",
			description: "Newest Codex model with the strongest reasoning.",
			recommended: false,
			effortLevels: ["low", "medium", "high", "xhigh", "max"],
		},
		{
			id: "gpt-5.4",
			label: "GPT-5.4",
			description: "Balanced default for agentic coding workflows.",
			recommended: true,
			effortLevels: ["low", "medium", "high", "xhigh", "max"],
		},
		{
			id: "gpt-5.4-mini",
			label: "GPT-5.4 Mini",
			description: "Fast, lightweight variant for quick tasks.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "gpt-5.3-codex",
			label: "GPT-5.3 Codex",
			description: "Previous-generation Codex with strong repo-aware reasoning.",
			recommended: false,
			effortLevels: ["low", "medium", "high", "xhigh"],
		},
	] satisfies ModelEntry[],

	gemini: [
		{
			id: "gemini-2.5-pro",
			label: "Gemini 2.5 Pro",
			description: "Stable long-context model with the broadest CLI compatibility.",
			recommended: true,
			effortLevels: ["low", "medium", "high", "xhigh"],
		},
		{
			id: "gemini-2.5-flash",
			label: "Gemini 2.5 Flash",
			description: "Fast stable variant.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "gemini-3-flash-preview",
			label: "Gemini 3 Flash Preview",
			description: "Preview Gemini 3 coding model. Availability may vary by account.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
	] satisfies ModelEntry[],

	droid: [
		{
			id: "auto",
			label: "Auto",
			description: "Use Droid's default model selection for this account.",
			recommended: true,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "claude-sonnet-4-6",
			label: "Claude Sonnet 4.6",
			description: "Balanced default with strong coding capability through Droid.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "gpt-5.4",
			label: "GPT-5.4",
			description: "High capability coding model routed through Droid.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "gpt-5.5",
			label: "GPT-5.5",
			description: "Latest high-reasoning OpenAI option exposed by Droid.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
		{
			id: "gemini-3-flash-preview",
			label: "Gemini 3 Flash Preview",
			description: "Fast Gemini option when available in the local Droid account.",
			recommended: false,
			effortLevels: ["low", "medium", "high"],
		},
	] satisfies ModelEntry[],

	cursor: [
		{
			id: "auto",
			label: "Auto",
			description: "Use Cursor's recommended model for this account.",
			recommended: true,
			effortLevels: ["low", "medium", "high"],
		},
	] satisfies ModelEntry[],
} as const;

export type ProviderRegistryKey = keyof typeof PROVIDER_MODEL_REGISTRY;

/**
 * Alias tables: short names and legacy date-versioned IDs that map to canonical model IDs.
 * When Anthropic / OpenAI / Google ships a new version:
 *   1. Add the new model to PROVIDER_MODEL_REGISTRY above.
 *   2. Add the old canonical ID here as an alias pointing to the new one.
 *   3. Users with stored configs are upgraded transparently.
 */
export const MODEL_ALIASES: Partial<Record<ProviderRegistryKey, Record<string, string>>> = {
	claude_code: {
		fable: "claude-fable-5",
		"fable-5": "claude-fable-5",
		opus: "claude-opus-4-8",
		"opus-4.8": "claude-opus-4-8",
		"opus-4.7": "claude-opus-4-8",
		"claude-opus-4-7": "claude-opus-4-8",
		"opus-4.6": "claude-opus-4-6",
		"claude-opus-4-6-20251117": "claude-opus-4-6",
		sonnet: "claude-sonnet-4-6",
		"sonnet-4.6": "claude-sonnet-4-6",
		"claude-sonnet-4-6-20251117": "claude-sonnet-4-6",
		haiku: "claude-haiku-4-5",
		"haiku-4.5": "claude-haiku-4-5",
		"claude-haiku-4-5-20251001": "claude-haiku-4-5",
	},
	codex: {
		"gpt-5-codex": "gpt-5.4",
		"5.5": "gpt-5.5",
		"5.4": "gpt-5.4",
		"5.4-mini": "gpt-5.4-mini",
		"5.3": "gpt-5.3-codex",
		"gpt-5.3": "gpt-5.3-codex",
	},
	gemini: {
		pro: "gemini-2.5-pro",
		flash: "gemini-2.5-flash",
		"3-flash-preview": "gemini-3-flash-preview",
		"gemini-3-flash-preview": "gemini-3-flash-preview",
		"3.1-pro": "gemini-2.5-pro",
		"3-flash": "gemini-2.5-flash",
		"gemini-3.1-pro": "gemini-2.5-pro",
		"gemini-3-flash": "gemini-2.5-flash",
		"2.5-pro": "gemini-2.5-pro",
		"2.5-flash": "gemini-2.5-flash",
	},
	droid: {
		auto: "auto",
		sonnet: "claude-sonnet-4-6",
		"claude-sonnet-4-6": "claude-sonnet-4-6",
		"gpt-5.4": "gpt-5.4",
		"5.4": "gpt-5.4",
		"gpt-5.5": "gpt-5.5",
		"5.5": "gpt-5.5",
		"gemini-3-flash-preview": "gemini-3-flash-preview",
	},
};

/**
 * Resolves a model alias or legacy ID to its canonical form.
 * Pass-through for IDs that are already canonical or unknown.
 */
export function resolveModelAlias(
	registryKey: ProviderRegistryKey,
	modelId: string | null | undefined,
): string | null {
	if (!modelId) return null;
	const trimmed = modelId.trim();
	if (!trimmed) return null;
	const canonical = MODEL_ALIASES[registryKey]?.[trimmed];
	return canonical ?? trimmed;
}

export function getDefaultModelId(registryKey: ProviderRegistryKey): string {
	const models = PROVIDER_MODEL_REGISTRY[registryKey] as ModelEntry[];
	return models.find((m) => m.recommended)?.id ?? models[0]?.id ?? "";
}
