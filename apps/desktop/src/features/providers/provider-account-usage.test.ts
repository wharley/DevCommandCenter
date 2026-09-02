import { describe, expect, it } from "vitest";
import type { ProviderAccountUsage, ProviderUsageWindow } from "@dcc/contracts";
import {
	mostConstrainedUsageWindow,
	providerUsageSeverity,
	supportsProviderAccountUsage,
} from "./provider-account-usage";

function window(remainingPercent: number): ProviderUsageWindow {
	return {
		id: String(remainingPercent),
		usedPercent: 100 - remainingPercent,
		remainingPercent,
		isExhausted: remainingPercent === 0,
	};
}

function usage(windows: ProviderUsageWindow[]): ProviderAccountUsage {
	return {
		providerId: "codex",
		state: "available",
		windows,
		updatedAt: "2026-07-15T12:00:00Z",
		isCached: false,
	};
}

describe("provider account usage", () => {
	it("derives usage support from the backend capability, never the provider id", () => {
		expect(
			supportsProviderAccountUsage({
				id: "codex",
				capabilities: { supportsAccountUsage: true },
			}),
		).toBe(true);
		expect(
			supportsProviderAccountUsage({
				id: "codex",
				capabilities: { supportsAccountUsage: false },
			}),
		).toBe(false);
		// Legacy catalogs without the field are treated as unsupported.
		expect(
			supportsProviderAccountUsage({ id: "claude_code", capabilities: {} }),
		).toBe(false);
		expect(supportsProviderAccountUsage(null)).toBe(false);
	});

	it("uses the window with the least remaining quota", () => {
		expect(
			mostConstrainedUsageWindow(usage([window(75), window(18)]))
				?.remainingPercent,
		).toBe(18);
	});

	it("prioritizes an exhausted window", () => {
		const exhausted = { ...window(30), isExhausted: true };
		expect(
			mostConstrainedUsageWindow(usage([window(10), exhausted]))?.isExhausted,
		).toBe(true);
	});

	it("only raises alerts at actionable thresholds", () => {
		expect(providerUsageSeverity(window(21))).toBeNull();
		expect(providerUsageSeverity(window(20))).toBe("warning");
		expect(providerUsageSeverity(window(5))).toBe("critical");
	});
});
