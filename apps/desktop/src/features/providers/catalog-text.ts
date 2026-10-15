import type { ProviderCatalog } from "@dcc/contracts";
import type { TFunction } from "i18next";

/**
 * The provider catalog comes from the backend in English. Settings speaks the
 * person's language, so known providers and model descriptions are said
 * through i18n; anything new or unknown still shows as the backend wrote it.
 */
const KNOWN_PROVIDERS = new Set([
	"claude_code",
	"codex",
	"cursor",
	"grok",
	"antigravity",
	"gemini",
	"droid",
]);

/** Exact backend model descriptions → their i18n key under catalogText.model. */
const MODEL_DESCRIPTION_KEYS: Record<string, string> = {
	"Most capable Claude model for demanding reasoning and long-horizon agentic work.": "claudeFable",
	"Everyday and complex tasks with a 1M-token context window.": "claudeOpus",
	"Most efficient for simpler tasks.": "efficientSimple",
	"Fastest and cheapest Claude model for quick follow-ups and subagent work.": "claudeHaiku",
	"Previous small model; use it on Claude Code older than 2.1.293.": "claudeHaikuPrevious",
	"Frontier intelligence for the most demanding work.": "frontier",
	"Latest workhorse model for coding and everyday work.": "latestWorkhorse",
	"Fast and affordable model for easier tasks.": "fastAffordable",
	"Previous generation workhorse model.": "previousWorkhorse",
	"Older generation workhorse model.": "olderWorkhorse",
	"Older balanced model for straightforward work.": "olderBalanced",
	"Older fast and efficient model.": "olderFast",
	"Legacy coding model.": "legacyCoding",
	"Use Cursor's recommended model for this account.": "cursorAuto",
	"Model reported by Cursor Agent CLI for this account.": "cursorReported",
	"Latest Grok Build model for coding, agentic tasks, and knowledge work through the local ACP CLI.": "grokLatest",
	"Use the default model offered to this Google account.": "antigravityDefault",
	"Model available to the authenticated Antigravity account.": "antigravityAccount",
	"Latest stable model for long-horizon coding and agentic workflows. CLI availability depends on the account rollout.": "geminiLatest",
	"Stable long-context model with the broadest CLI compatibility.": "geminiStable",
	"Use Droid's default model selection for this account.": "droidAuto",
	"Fast Gemini option when available in the local Droid account.": "droidGeminiFast",
	"High capability coding model routed through Droid.": "droidHighCapability",
	"Latest high-reasoning OpenAI option exposed by Droid.": "droidOpenAiLatest",
};

export function providerDescriptionText(
	provider: ProviderCatalog["providers"][number],
	t: TFunction<"common">,
): string {
	return KNOWN_PROVIDERS.has(provider.id)
		? t(`catalogText.provider.${provider.id}`, { defaultValue: provider.description })
		: provider.description;
}

export function modelDescriptionText(description: string, t: TFunction<"common">): string {
	const key = MODEL_DESCRIPTION_KEYS[description.trim()];
	return key ? t(`catalogText.model.${key}`, { defaultValue: description }) : description;
}
