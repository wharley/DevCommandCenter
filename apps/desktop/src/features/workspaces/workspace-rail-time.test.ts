import { describe, expect, it } from "vitest";
import {
	formatCompactElapsedTime,
	workspaceActivityTimestamp,
} from "./workspace-rail-time";

describe("workspace rail time", () => {
	it("formats a compact running duration", () => {
		expect(formatCompactElapsedTime(42_900)).toBe("42s");
		expect(formatCompactElapsedTime(5 * 60_000)).toBe("5min");
		expect(formatCompactElapsedTime(2 * 60 * 60_000 + 7 * 60_000)).toBe(
			"2h 7min",
		);
	});

	it("uses the start while active and completion after settling", () => {
		expect(
			workspaceActivityTimestamp({
				state: "active",
				startedAt: "2026-07-24T12:00:00.000Z",
				completedAt: null,
			}),
		).toBe("2026-07-24T12:00:00.000Z");
		expect(
			workspaceActivityTimestamp({
				state: "completed",
				startedAt: "2026-07-24T12:00:00.000Z",
				completedAt: "2026-07-24T12:05:00.000Z",
			}),
		).toBe("2026-07-24T12:05:00.000Z");
	});
});
