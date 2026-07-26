import { describe, expect, it } from "vitest";
import {
	formatCompactElapsedTime,
	workspaceActivityTimestamp,
} from "./workspace-rail-time";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

describe("workspace rail time", () => {
	it("formats a compact running duration", () => {
		expect(formatCompactElapsedTime(42_900)).toBe("42s");
		expect(formatCompactElapsedTime(5 * 60_000)).toBe("5min");
		expect(formatCompactElapsedTime(2 * 60 * 60_000 + 7 * 60_000)).toBe(
			"2h 7min",
		);
	});

	it("rolls up to days and months instead of piling up hours", () => {
		expect(formatCompactElapsedTime(23 * HOUR + 59 * MINUTE)).toBe("23h 59min");
		expect(formatCompactElapsedTime(DAY)).toBe("1d");
		expect(formatCompactElapsedTime(DAY + 5 * HOUR + 30 * MINUTE)).toBe("1d 5h");
		expect(formatCompactElapsedTime(609 * HOUR + 30 * MINUTE)).toBe("25d 9h");
		expect(formatCompactElapsedTime(30 * DAY)).toBe("1mo");
		expect(formatCompactElapsedTime(34 * DAY + 3 * HOUR)).toBe("1mo 4d");
		expect(formatCompactElapsedTime(75 * DAY)).toBe("2mo 15d");
	});

	it("uses the provided unit labels", () => {
		const ptBR = {
			second: "s",
			minute: "min",
			hour: "h",
			day: "d",
			month: "mês",
			months: "meses",
		};
		expect(formatCompactElapsedTime(31 * DAY, ptBR)).toBe("1mês 1d");
		expect(formatCompactElapsedTime(60 * DAY, ptBR)).toBe("2meses");
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
