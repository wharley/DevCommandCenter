import type { WorkspaceSummary } from "./types";

export type SnoozeOptionId = "hour" | "tomorrow" | "monday";

/** When each snooze choice ends, in the person's local time. */
export function snoozeUntil(option: SnoozeOptionId, now: Date): Date {
	if (option === "hour") {
		return new Date(now.getTime() + 60 * 60 * 1000);
	}
	const at = new Date(now);
	at.setHours(9, 0, 0, 0);
	if (option === "tomorrow") {
		at.setDate(at.getDate() + 1);
		return at;
	}
	// Next Monday 09:00; on a Monday it is the following week's.
	const daysUntilMonday = ((8 - at.getDay()) % 7) || 7;
	at.setDate(at.getDate() + daysUntilMonday);
	return at;
}

function snoozedUntilMs(workspace: Pick<WorkspaceSummary, "snoozedUntil">): number | null {
	const at = workspace.snoozedUntil ? Date.parse(workspace.snoozedUntil) : Number.NaN;
	return Number.isNaN(at) ? null : at;
}

/** Still snoozed: kept in "On hold" until the instant passes. */
export function isSnoozed(workspace: Pick<WorkspaceSummary, "snoozedUntil">, now: number): boolean {
	const at = snoozedUntilMs(workspace);
	return at !== null && at > now;
}

/** The snooze ended and the person has not opened the task since. */
export function hasWoken(workspace: Pick<WorkspaceSummary, "snoozedUntil">, now: number): boolean {
	const at = snoozedUntilMs(workspace);
	return at !== null && at <= now;
}
