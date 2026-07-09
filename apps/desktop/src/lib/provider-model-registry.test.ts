import { describe, expect, it } from "vitest";
import {
	FALLBACK_PROVIDER_CATALOG,
} from "./fallback-provider-catalog";
import {
	getDefaultModelId,
	PROVIDER_MODEL_REGISTRY,
	resolveModelAlias,
} from "./provider-model-registry";

describe("provider-model-registry", () => {
	it("resolves Claude Fable aliases to Claude Fable 5", () => {
		expect(resolveModelAlias("claude_code", "fable")).toBe("claude-fable-5");
		expect(resolveModelAlias("claude_code", "fable-5")).toBe("claude-fable-5");
	});

	it("upgrades Claude Opus 4.7 aliases to Claude Opus 4.8", () => {
		expect(resolveModelAlias("claude_code", "opus")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "opus-4.8")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "opus-4.7")).toBe("claude-opus-4-8");
		expect(resolveModelAlias("claude_code", "claude-opus-4-7")).toBe(
			"claude-opus-4-8",
		);
	});

	it("keeps Claude Sonnet 5 as the default Claude model", () => {
		expect(getDefaultModelId("claude_code")).toBe("claude-sonnet-5");
	});

	it("upgrades Claude Sonnet aliases to Claude Sonnet 5", () => {
		expect(resolveModelAlias("claude_code", "sonnet")).toBe("claude-sonnet-5");
		expect(resolveModelAlias("claude_code", "sonnet-5")).toBe("claude-sonnet-5");
		expect(resolveModelAlias("claude_code", "sonnet-4.6")).toBe("claude-sonnet-5");
		expect(resolveModelAlias("claude_code", "claude-sonnet-4-6")).toBe(
			"claude-sonnet-5",
		);
	});

	it("resolves Droid aliases to canonical IDs", () => {
		expect(resolveModelAlias("droid", "auto")).toBe("auto");
		expect(resolveModelAlias("droid", "sonnet")).toBe("claude-sonnet-5");
		expect(resolveModelAlias("droid", "5.4")).toBe("gpt-5.4");
	});

	it("registers the GPT-5.6 Codex preview models", () => {
		const models = PROVIDER_MODEL_REGISTRY.codex;
		expect(models.map((model) => model.id)).toEqual(
			expect.arrayContaining(["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"]),
		);
		expect(resolveModelAlias("codex", "sol")).toBe("gpt-5.6-sol");
		expect(resolveModelAlias("codex", "5.6-terra")).toBe("gpt-5.6-terra");
		expect(resolveModelAlias("codex", "luna")).toBe("gpt-5.6-luna");
	});

	it("registers Grok 4.5 and resolves its aliases", () => {
		expect(getDefaultModelId("grok")).toBe("grok-4.5");
		expect(resolveModelAlias("grok", "grok")).toBe("grok-4.5");
		expect(resolveModelAlias("grok", "4.5")).toBe("grok-4.5");
		const provider = FALLBACK_PROVIDER_CATALOG.providers.find(
			(candidate) => candidate.id === "grok",
		);
		expect(provider?.models[0]?.id).toBe("grok-4.5");
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
