import { describe, expect, it } from "vitest";
import {
	getProviderActionBlockReason,
	getProviderUnhealthyReason,
	isProviderEnabled,
	resolveSelectedModelId,
	resolveSelectedProviderId,
} from "./provider-selection.logic";

function makeProvider(id: string, stable: boolean, enabled?: boolean) {
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
			mcpSupport: "unsupported" as const,
			resumable: false,
			vision: false,
			experimental: false,
			canBeDelegationTarget: true,
			canRequestDelegation: false,
			supportsReadOnlyDelegation: true,
			supportsEditDelegation: true,
		},
		health: "Healthy" as const,
		...(enabled === undefined ? {} : { enabled }),
	};
}

describe("isProviderEnabled", () => {
	it("defaults legacy catalog entries to enabled", () => {
		expect(isProviderEnabled(makeProvider("legacy", true))).toBe(true);
	});

	it("recognizes an explicitly disabled provider", () => {
		expect(isProviderEnabled(makeProvider("disabled", true, false))).toBe(false);
	});

	it("blocks new work for disabled or missing providers", () => {
		const disabled = makeProvider("disabled", true, false);
		expect(getProviderActionBlockReason(disabled, "disabled", "missing")).toBe(
			"disabled",
		);
		expect(getProviderActionBlockReason(makeProvider("enabled", true), "disabled", "missing")).toBeNull();
		expect(getProviderActionBlockReason(null, "disabled", "missing")).toBe("missing");
	});
});

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

	it("preserves a disabled current selection so its draft can be restored", () => {
		expect(
			resolveSelectedProviderId(
				[makeProvider("disabled", true, false), makeProvider("beta", false)],
				"disabled",
			),
		).toBe("disabled");
	});

	it("does not default new work to a disabled provider", () => {
		expect(
			resolveSelectedProviderId(
				[makeProvider("disabled", true, false), makeProvider("beta", false)],
				null,
			),
		).toBe("beta");
	});

	it("returns null when every provider is disabled", () => {
		expect(
			resolveSelectedProviderId(
				[makeProvider("alpha", true, false), makeProvider("beta", false, false)],
				null,
			),
		).toBeNull();
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
				id: "claude-opus-5",
				label: "Claude Opus 5",
				description: "",
				recommended: false,
				effortLevels: ["low", "medium", "high", "xhigh", "max"],
			},
			{
				id: "claude-sonnet-5",
				label: "Claude Sonnet 5",
				description: "",
				recommended: true,
				effortLevels: ["low", "medium", "high", "xhigh", "max"],
			},
		],
		stable: true,
		capabilities: {
			streaming: false,
			tools: false,
			mcpSupport: "unsupported" as const,
			resumable: false,
			vision: false,
			experimental: false,
			canBeDelegationTarget: true,
			canRequestDelegation: false,
			supportsReadOnlyDelegation: true,
			supportsEditDelegation: true,
		},
		health: "Healthy" as const,
	};

	it("keeps a stored model when it still exists", () => {
		expect(resolveSelectedModelId(provider, "alpha-default")).toBe("alpha-default");
	});

	it("upgrades legacy Claude Opus selections to Claude Opus 5", () => {
		expect(resolveSelectedModelId(claudeProvider, "opus-4.7")).toBe(
			"claude-opus-5",
		);
		expect(resolveSelectedModelId(claudeProvider, "claude-opus-4-7")).toBe(
			"claude-opus-5",
		);
	});

	it("upgrades legacy Claude Sonnet selections to Claude Sonnet 5", () => {
		expect(resolveSelectedModelId(claudeProvider, "sonnet-4.6")).toBe(
			"claude-sonnet-5",
		);
		expect(resolveSelectedModelId(claudeProvider, "claude-sonnet-4-6")).toBe(
			"claude-sonnet-5",
		);
	});

	it("prefers the recommended model when the stored one is invalid", () => {
		expect(resolveSelectedModelId(provider, "missing")).toBe("alpha-default");
	});

	it("keeps Claude Sonnet 5 as the recommended fallback", () => {
		expect(resolveSelectedModelId(claudeProvider, "missing")).toBe(
			"claude-sonnet-5",
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
