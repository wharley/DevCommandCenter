import { describe, expect, it } from "vitest";
import {
	getProviderUnhealthyReason,
	resolveSelectedModelId,
	resolveSelectedProviderId,
} from "./provider-selection.logic";

function makeProvider(id: string, stable: boolean) {
	return {
		id,
		label: id.toUpperCase(),
		description: "",
		models: [
			{
				id: `${id}-default`,
				label: `${id.toUpperCase()} Default`,
				description: "",
				recommended: true,
				effortLevels: ["low", "balanced", "high"],
			},
		],
		stable,
		capabilities: {
			streaming: false,
			tools: false,
			mcp: false,
			resumable: false,
			vision: false,
			experimental: false,
		},
		health: "Healthy" as const,
	};
}

describe("resolveSelectedProviderId", () => {
	it("keeps a stored provider when it still exists", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", true)], "alpha"),
		).toBe("alpha");
	});

	it("prefers the stable provider when no stored provider is valid", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", true)], "missing"),
		).toBe("beta");
	});

	it("falls back to the first provider when there is no stable one", () => {
		expect(
			resolveSelectedProviderId([makeProvider("alpha", false), makeProvider("beta", false)], null),
		).toBe("alpha");
	});

	it("returns null when there are no providers", () => {
		expect(resolveSelectedProviderId([], null)).toBeNull();
	});
});

describe("resolveSelectedModelId", () => {
	const provider = makeProvider("alpha", true);
	const claudeProvider = {
		id: "claude_code",
		label: "Claude Code",
		description: "",
		models: [
			{
				id: "claude-fable-5",
				label: "Claude Fable 5",
				description: "",
				recommended: false,
				effortLevels: ["low", "medium", "high", "xhigh", "max"],
			},
			{
				id: "claude-opus-4-8",
				label: "Claude Opus 4.8",
				description: "",
				recommended: false,
				effortLevels: ["low", "medium", "high", "xhigh", "max"],
			},
			{
				id: "claude-sonnet-4-6",
				label: "Claude Sonnet 4.6",
				description: "",
				recommended: true,
				effortLevels: ["low", "medium", "high", "xhigh"],
			},
		],
		stable: true,
		capabilities: {
			streaming: false,
			tools: false,
			mcp: false,
			resumable: false,
			vision: false,
			experimental: false,
		},
		health: "Healthy" as const,
	};

	it("keeps a stored model when it still exists", () => {
		expect(resolveSelectedModelId(provider, "alpha-default")).toBe("alpha-default");
	});

	it("upgrades legacy Claude Opus 4.7 selections to Claude Opus 4.8", () => {
		expect(resolveSelectedModelId(claudeProvider, "opus-4.7")).toBe(
			"claude-opus-4-8",
		);
		expect(resolveSelectedModelId(claudeProvider, "claude-opus-4-7")).toBe(
			"claude-opus-4-8",
		);
	});

	it("prefers the recommended model when the stored one is invalid", () => {
		expect(resolveSelectedModelId(provider, "missing")).toBe("alpha-default");
	});

	it("keeps Claude Sonnet 4.6 as the recommended fallback", () => {
		expect(resolveSelectedModelId(claudeProvider, "missing")).toBe(
			"claude-sonnet-4-6",
		);
	});

	it("returns null when the provider has no models", () => {
		expect(resolveSelectedModelId({ ...provider, models: [] }, "missing")).toBeNull();
	});
});

describe("getProviderUnhealthyReason", () => {
	it("returns null for healthy providers", () => {
		expect(getProviderUnhealthyReason(makeProvider("alpha", true))).toBeNull();
	});

	it("returns the unhealthy reason when the provider cannot send", () => {
		expect(
			getProviderUnhealthyReason({
				...makeProvider("alpha", true),
				health: { Unhealthy: { reason: "Cursor Agent is not authenticated." } },
			}),
		).toBe("Cursor Agent is not authenticated.");
	});
});
