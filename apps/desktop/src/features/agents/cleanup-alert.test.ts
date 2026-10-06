import { describe, expect, it } from "vitest";
import {
	CLEANUP_REALERT_GROWTH_BYTES,
	type CleanupAlertState,
	clampCleanupThresholdGb,
	isCleanupAlertDue,
	settleCleanupAlert,
} from "./cleanup-alert";

const GIB = 1024 ** 3;
const state = (overrides: Partial<CleanupAlertState> = {}): CleanupAlertState => ({
	enabled: true,
	thresholdGb: 5,
	dismissedAtBytes: null,
	...overrides,
});

describe("isCleanupAlertDue", () => {
	it("speaks up once completed tasks reach the limit", () => {
		expect(isCleanupAlertDue(state(), 4.9 * GIB)).toBe(false);
		expect(isCleanupAlertDue(state(), 5 * GIB)).toBe(true);
	});

	it("stays quiet when the person turned it off", () => {
		expect(isCleanupAlertDue(state({ enabled: false }), 50 * GIB)).toBe(false);
	});

	it("after a close, comes back only when the total grows by another step", () => {
		const closed = state({ dismissedAtBytes: 8 * GIB });
		expect(isCleanupAlertDue(closed, 8.5 * GIB)).toBe(false);
		expect(isCleanupAlertDue(closed, 8 * GIB + CLEANUP_REALERT_GROWTH_BYTES)).toBe(true);
	});
});

describe("settleCleanupAlert", () => {
	it("forgets a close once the total drops under the limit", () => {
		const closed = state({ dismissedAtBytes: 8 * GIB });
		expect(settleCleanupAlert(closed, 2 * GIB).dismissedAtBytes).toBeNull();
		expect(settleCleanupAlert(closed, 6 * GIB)).toBe(closed);
	});
});

describe("clampCleanupThresholdGb", () => {
	it("keeps the limit to whole gigabytes within range", () => {
		expect(clampCleanupThresholdGb(0)).toBe(1);
		expect(clampCleanupThresholdGb(7.4)).toBe(7);
		expect(clampCleanupThresholdGb(Number.NaN)).toBe(5);
		expect(clampCleanupThresholdGb(5000)).toBe(1000);
	});
});
