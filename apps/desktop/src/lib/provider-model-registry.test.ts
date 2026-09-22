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
	it("resolves current and legacy Claude Fable aliases to Claude Fable 5.1", () => {
		expect(resolveModelAlias("claude_code", "fable")).toBe("claude-fable-5-1");
		expect(resolveModelAlias("claude_code", "fable-5.1")).toBe(
			"claude-fable-5-1",
		);
		expect(resolveModelAlias("claude_code", "fable-5")).toBe(
			"claude-fable-5-1",
		);
		expect(resolveModelAlias("claude_code", "claude-fable-5")).toBe(
			"claude-fable-5-1",
		);
		expect(PROVIDER_MODEL_REGISTRY.claude_code.map((model) => model.id)).not.toContain(
			"claude-fable-5",
		);
	});

	it.each([
		"opus",
		"opus-5.5",
		"opus-5-5",
		"claude-opus-5-5",
		"opus-5",
		"claude-opus-5",
		"opus-4.8",
		"claude-opus-4-8",
		"opus-4.7",
		"claude-opus-4-7",
		"opus-4.6",
		"claude-opus-4-6",
		"claude-opus-4-6-20251117",
	])("resolves Claude Opus alias %s to Claude Opus 5.5", (alias) => {
		expect(resolveModelAlias("claude_code", alias)).toBe("claude-opus-5-5");
	});

	it("registers Claude Opus 5.5 with its full effort ladder", () => {
		const opus = PROVIDER_MODEL_REGISTRY.claude_code.find(
			(model) => model.id === "claude-opus-5-5",
		);
		expect(opus?.effortLevels).toEqual(["low", "medium", "high", "xhigh", "max"]);
		expect(PROVIDER_MODEL_REGISTRY.claude_code.map((model) => model.id)).not.toContain(
			"claude-opus-5",
		);
	});

	it("recommends Claude Opus 5.5 as the default Claude model", () => {
		expect(getDefaultModelId("claude_code")).toBe("claude-opus-5-5");
		expect(
			PROVIDER_MODEL_REGISTRY.claude_code.filter((model) => model.recommended),
		).toHaveLength(1);
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

	it("matches the current Codex picker and keeps Astra as the default", () => {
		const models = PROVIDER_MODEL_REGISTRY.codex;
		expect(models.map((model) => model.id)).toEqual([
			"gpt-6-astra",
			"gpt-6-sol",
			"gpt-6-luna",
			"gpt-5.6-sol",
			"gpt-5.6-terra",
			"gpt-5.6-luna",
			"gpt-5.5",
		]);
		expect(getDefaultModelId("codex")).toBe("gpt-6-astra");
		expect(resolveModelAlias("codex", "astra")).toBe("gpt-6-astra");
		expect(resolveModelAlias("codex", "6-astra")).toBe("gpt-6-astra");
		for (const id of ["gpt-6-astra", "gpt-6-sol", "gpt-6-luna"]) {
			expect(models.find((model) => model.id === id)?.effortLevels).toEqual(
				["low", "medium", "high", "xhigh", "max"],
			);
		}
		expect(resolveModelAlias("codex", "sol")).toBe("gpt-6-sol");
		expect(resolveModelAlias("codex", "6-sol")).toBe("gpt-6-sol");
		expect(resolveModelAlias("codex", "5.6-sol")).toBe("gpt-5.6-sol");
		expect(resolveModelAlias("codex", "5.6-terra")).toBe("gpt-5.6-terra");
		expect(resolveModelAlias("codex", "luna")).toBe("gpt-6-luna");
		expect(resolveModelAlias("codex", "6-luna")).toBe("gpt-6-luna");
		expect(resolveModelAlias("codex", "5.6-luna")).toBe("gpt-5.6-luna");
		expect(
			FALLBACK_PROVIDER_CATALOG.providers.find((provider) => provider.id === "codex")?.models,
		).toEqual(models);
	});

	it.each([
		["gpt-5-codex", "gpt-6-sol"],
		["gpt-5.4", "gpt-6-sol"],
		["gpt-5.4-mini", "gpt-6-luna"],
		["gpt-5.3-codex", "gpt-6-sol"],
		["gpt-5.3-codex-spark", "gpt-6-luna"],
	])("resolves retired Codex selection %s to %s", (legacy, replacement) => {
		expect(resolveModelAlias("codex", legacy)).toBe(replacement);
		expect(
			PROVIDER_MODEL_REGISTRY.codex.some((model) => model.id === replacement),
		).toBe(true);
		expect(
			PROVIDER_MODEL_REGISTRY.codex.some((model) => model.id === legacy),
		).toBe(false);
	});

	it("keeps the Gemini catalog focused on 3.8 Flash and the 2.5 Pro fallback", () => {
		expect(PROVIDER_MODEL_REGISTRY.gemini.map((model) => model.id)).toEqual([
			"gemini-3.8-flash",
			"gemini-2.5-pro",
		]);
		expect(getDefaultModelId("gemini")).toBe("gemini-3.8-flash");
		expect(resolveModelAlias("gemini", "flash")).toBe("gemini-3.8-flash");
		expect(resolveModelAlias("gemini", "gemini-2.5-flash")).toBe(
			"gemini-3.8-flash",
		);
		expect(resolveModelAlias("gemini", "gemini-3-flash-preview")).toBe(
			"gemini-3.8-flash",
		);
	});

	it("registers Grok 4.7 through its official Grok Build model ID", () => {
		expect(getDefaultModelId("grok")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "grok")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "4.7")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "grok-4-7")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "grok-build")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "4.6")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "grok-4.6")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "4.5")).toBe("grok-4.7");
		expect(resolveModelAlias("grok", "grok-4.5")).toBe("grok-4.7");
		const provider = FALLBACK_PROVIDER_CATALOG.providers.find(
			(candidate) => candidate.id === "grok",
		);
		expect(provider?.models[0]?.id).toBe("grok-4.7");
		expect(provider?.models[0]?.label).toBe("Grok 4.7");
		expect(provider?.models[0]?.effortLevels).toEqual([
			"low",
			"medium",
			"high",
			"xhigh",
		]);
		expect(provider?.stable).toBe(true);
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
