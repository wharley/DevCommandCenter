import type { WorkspaceAgentActivity } from "./use-workspace-agent-states";

export type ElapsedUnitLabels = {
	second: string;
	minute: string;
	hour: string;
	day: string;
	month: string;
	months: string;
};

export const DEFAULT_ELAPSED_UNIT_LABELS: ElapsedUnitLabels = {
	second: "s",
	minute: "min",
	hour: "h",
	day: "d",
	month: "mo",
	months: "mo",
};

const MINUTE_SECONDS = 60;
const HOUR_SECONDS = 60 * MINUTE_SECONDS;
const DAY_SECONDS = 24 * HOUR_SECONDS;
const MONTH_DAYS = 30;

export function formatCompactElapsedTime(
	elapsedMs: number,
	labels: ElapsedUnitLabels = DEFAULT_ELAPSED_UNIT_LABELS,
): string {
	const totalSeconds = Number.isFinite(elapsedMs)
		? Math.max(0, Math.floor(elapsedMs / 1_000))
		: 0;
	if (totalSeconds < MINUTE_SECONDS) {
		return `${totalSeconds}${labels.second}`;
	}

	const totalMinutes = Math.floor(totalSeconds / MINUTE_SECONDS);
	if (totalMinutes < 60) {
		return `${totalMinutes}${labels.minute}`;
	}

	if (totalSeconds < DAY_SECONDS) {
		const hours = Math.floor(totalMinutes / 60);
		const minutes = totalMinutes % 60;
		return minutes > 0
			? `${hours}${labels.hour} ${minutes}${labels.minute}`
			: `${hours}${labels.hour}`;
	}

	const totalDays = Math.floor(totalSeconds / DAY_SECONDS);
	if (totalDays < MONTH_DAYS) {
		const hours = Math.floor((totalSeconds % DAY_SECONDS) / HOUR_SECONDS);
		return hours > 0
			? `${totalDays}${labels.day} ${hours}${labels.hour}`
			: `${totalDays}${labels.day}`;
	}

	const months = Math.floor(totalDays / MONTH_DAYS);
	const days = totalDays % MONTH_DAYS;
	const monthLabel = months === 1 ? labels.month : labels.months;
	return days > 0
		? `${months}${monthLabel} ${days}${labels.day}`
		: `${months}${monthLabel}`;
}

export function workspaceActivityTimestamp(
	activity: WorkspaceAgentActivity,
): string | null {
	return activity.state === "active"
		? activity.startedAt
		: activity.completedAt;
}
