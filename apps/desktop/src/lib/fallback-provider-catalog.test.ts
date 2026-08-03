import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "./fallback-provider-catalog";

describe("fallback provider approval policies", () => {
	it.each(["claude_code", "codex", "gemini"])(
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
