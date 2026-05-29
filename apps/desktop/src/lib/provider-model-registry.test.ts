import { describe, expect, it } from "vitest";
import {
	FALLBACK_PROVIDER_CATALOG,
} from "./fallback-provider-catalog";
import {
	getDefaultModelId,
	resolveModelAlias,
} from "./provider-model-registry";

describe("provider-model-registry", () => {
	it("upgrades Claude Opus 4.7 aliases to Claude Opus 4.8", () => {
		expect(resolveModelAlias("claude_code", "opus")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "opus-4.8")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "opus-4.7")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "claude-opus-4-7")).toBe(
			"claude-opus-4-8",
		);
	});

	it("resolves Droid aliases to canonical IDs", () => {
		expect(resolveModelAlias("droid", "auto")).toBe("auto");
		expect(resolveModelAlias("droid", "sonnet")).toBe("claude-sonnet-4-6");
		expect(resolveModelAlias("droid", "5.4")).toBe("gpt-5.4");
	});

	it("uses Auto as the default Droid model", () => {
		expect(getDefaultModelId("droid")).toBe("auto");
	});

	it("includes Droid in the fallback provider catalog", () => {
		const provider = FALLBACK_PROVIDER_CATALOG.providers.find(
			(candidate) => candidate.id === "droid",
		);
		expect(provider).toBeTruthy();
		expect(provider?.stable).toBe(true);
		expect(provider?.models[0]?.id).toBe("auto");
	});
});
