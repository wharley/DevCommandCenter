import type { ProviderCatalog } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import {
	buildDelegateTaskToolInstructions,
	resolveDelegateTaskToolInstructions,
} from "./delegate-task-tool-instructions";

describe("delegate task tool instructions", () => {
	it("routes Claude-family requests through the native Agent tool", () => {
		const instructions = buildDelegateTaskToolInstructions(
			FALLBACK_PROVIDER_CATALOG.providers,
			"claude_code",
		);

		expect(instructions).toContain("use Claude's native Agent tool");
		expect(instructions).toContain('"Opus 5" means Agent.model = "opus"');
		expect(instructions).toContain(
			"delegate_task is exclusively for delegation to a different provider",
		);
		expect(instructions).toContain("Available external-provider delegation targets:");
		expect(instructions).not.toContain("claude_code (Claude Code)");
	});

	it("routes Codex model requests through native spawn_agent", () => {
		const instructions = buildDelegateTaskToolInstructions(
			FALLBACK_PROVIDER_CATALOG.providers,
			"codex",
		);

		expect(instructions).toContain("use Codex's native spawn_agent tool");
		expect(instructions).toContain("pass the requested model ID");
		expect(instructions).not.toContain("codex (Codex)");
	});

	it("does not expose instructions to providers that cannot request delegation", () => {
		const provider = {
			...FALLBACK_PROVIDER_CATALOG.providers[0],
			capabilities: {
				...FALLBACK_PROVIDER_CATALOG.providers[0]!.capabilities,
				canRequestDelegation: false,
			},
		} as ProviderCatalog["providers"][number];

		expect(
			resolveDelegateTaskToolInstructions({ provider, providers: [provider] }),
		).toBeNull();
	});
});
