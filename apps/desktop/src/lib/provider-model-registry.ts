export type ModelEntry = {
	id: string;
	label: string;
	description: string;
	recommended: boolean;
};

/**
 * Single source of truth for provider model lists.
 * Keys match the Rust provider IDs (e.g. "claude_code").
 * To add a new model version, update only this file.
 */
export const PROVIDER_MODEL_REGISTRY = {
	claude_code: [
		{
			id: "claude-opus-4-7",
			label: "Claude Opus 4.7",
			description: "Highest capability, best for deep reasoning and large refactors.",
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
	] satisfies ModelEntry[],

	codex: [
		{
			id: "gpt-5.5",
			label: "GPT-5.5",
			description: "Newest Codex model with the strongest reasoning.",
			recommended: false,
		},
		{
			id: "gpt-5.4",
			label: "GPT-5.4",
			description: "Balanced default for agentic coding workflows.",
			recommended: true,
		},
		{
			id: "gpt-5.4-mini",
			label: "GPT-5.4 Mini",
			description: "Fast, lightweight variant for quick tasks.",
			recommended: false,
		},
		{
			id: "gpt-5.3-codex",
			label: "GPT-5.3 Codex",
			description: "Previous-generation Codex with strong repo-aware reasoning.",
			recommended: false,
		},
	] satisfies ModelEntry[],

	gemini: [
		{
			id: "gemini-3.1-pro",
			label: "Gemini 3.1 Pro",
			description: "Latest Gemini model with extended context and reasoning.",
			recommended: true,
		},
		{
			id: "gemini-3-flash",
			label: "Gemini 3 Flash",
			description: "Fast Gemini 3 variant for high-throughput tasks.",
			recommended: false,
		},
		{
			id: "gemini-2.5-pro",
			label: "Gemini 2.5 Pro",
			description: "Stable long-context model.",
			recommended: false,
		},
		{
			id: "gemini-2.5-flash",
			label: "Gemini 2.5 Flash",
			description: "Fast stable variant.",
			recommended: false,
		},
	] satisfies ModelEntry[],

	cursor: [
		{
			id: "auto",
			label: "Auto",
			description: "Use Cursor's recommended model for this account.",
			recommended: true,
		},
	] satisfies ModelEntry[],
} as const;

export type ProviderRegistryKey = keyof typeof PROVIDER_MODEL_REGISTRY;

export function getDefaultModelId(registryKey: ProviderRegistryKey): string {
	const models = PROVIDER_MODEL_REGISTRY[registryKey] as ModelEntry[];
	return models.find((m) => m.recommended)?.id ?? models[0]?.id ?? "";
}
