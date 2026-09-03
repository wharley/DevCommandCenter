import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "./fallback-provider-catalog";

describe("fallback provider catalog", () => {
	it("keeps the provider picker in product order", () => {
		expect(
			FALLBACK_PROVIDER_CATALOG.providers.map((provider) => provider.id),
		).toEqual([
			"claude_code",
			"codex",
			"cursor",
			"grok",
			"antigravity",
			"gemini",
			"droid",
		]);
	});

	it.each(["claude_code", "codex", "antigravity", "gemini"])(
		"keeps %s approval controls available while the native catalog loads",
		(providerId) => {
			const provider = FALLBACK_PROVIDER_CATALOG.providers.find(
				(candidate) => candidate.id === providerId,
			);

			expect(provider?.capabilities.approvalPolicies).toEqual([
				"ask",
				"auto",
				"full_access",
			]);
		},
	);
});
