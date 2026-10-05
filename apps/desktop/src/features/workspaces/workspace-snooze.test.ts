import { describe, expect, it } from "vitest";
import { hasWoken, isSnoozed, snoozeUntil } from "./workspace-snooze";

describe("workspace snooze", () => {
	// Wednesday 2026-10-07 14:30 local time.
	const now = new Date(2026, 9, 7, 14, 30);

	it("computes the end of each choice in local time", () => {
		expect(snoozeUntil("hour", now).getTime()).toBe(now.getTime() + 3_600_000);
		const tomorrow = snoozeUntil("tomorrow", now);
		expect([tomorrow.getDate(), tomorrow.getHours(), tomorrow.getMinutes()]).toEqual([8, 9, 0]);
		const monday = snoozeUntil("monday", now);
		expect([monday.getDay(), monday.getDate(), monday.getHours()]).toEqual([1, 12, 9]);
		// On a Monday, "Monday" means next week.
		expect(snoozeUntil("monday", new Date(2026, 9, 12, 8, 0)).getDate()).toBe(19);
	});

	it("tells snoozed from woken tasks", () => {
		const at = new Date(2026, 9, 7, 15, 0).toISOString();
		expect(isSnoozed({ snoozedUntil: at }, now.getTime())).toBe(true);
		expect(hasWoken({ snoozedUntil: at }, now.getTime())).toBe(false);
		const later = new Date(2026, 9, 7, 16, 0).getTime();
		expect(isSnoozed({ snoozedUntil: at }, later)).toBe(false);
		expect(hasWoken({ snoozedUntil: at }, later)).toBe(true);
		expect(isSnoozed({ snoozedUntil: null }, later)).toBe(false);
		expect(hasWoken({ snoozedUntil: null }, later)).toBe(false);
	});
});
