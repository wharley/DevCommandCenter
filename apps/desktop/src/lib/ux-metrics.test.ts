import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	DCC_UX_METRICS_STORAGE_KEY,
	readUxMetrics,
	recordUxMetric,
} from "./ux-metrics";

function createStorage() {
	const values = new Map<string, string>();
	return {
		getItem: (key: string) => values.get(key) ?? null,
		setItem: (key: string, value: string) => values.set(key, value),
		removeItem: (key: string) => values.delete(key),
	};
}

describe("UX metrics", () => {
	beforeEach(() => {
		vi.stubGlobal("window", { localStorage: createStorage() });
	});

	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it("stores only aggregate counts and elapsed times", () => {
		recordUxMetric("terminal_discovered");
		recordUxMetric("terminal_discovered");

		const metric = readUxMetrics().terminal_discovered;
		expect(metric?.count).toBe(2);
		expect(metric?.firstElapsedMs).toBeGreaterThanOrEqual(0);
		expect(metric?.lastElapsedMs).toBeGreaterThanOrEqual(
			metric?.firstElapsedMs ?? 0,
		);
		const stored = window.localStorage.getItem(DCC_UX_METRICS_STORAGE_KEY) ?? "";
		expect(stored).not.toContain("prompt");
		expect(stored).not.toContain("path");
	});

	it("recovers from malformed local storage", () => {
		window.localStorage.setItem(DCC_UX_METRICS_STORAGE_KEY, "not-json");
		expect(readUxMetrics()).toEqual({});
	});
});
