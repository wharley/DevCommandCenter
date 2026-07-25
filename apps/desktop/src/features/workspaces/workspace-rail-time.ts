import type { WorkspaceAgentActivity } from "./use-workspace-agent-states";

export function formatCompactElapsedTime(elapsedMs: number): string {
	const totalSeconds = Number.isFinite(elapsedMs)
		? Math.max(0, Math.floor(elapsedMs / 1_000))
		: 0;
	if (totalSeconds < 60) {
		return `${totalSeconds}s`;
	}

	const totalMinutes = Math.floor(totalSeconds / 60);
	if (totalMinutes < 60) {
		return `${totalMinutes}min`;
	}

	const hours = Math.floor(totalMinutes / 60);
	const minutes = totalMinutes % 60;
	return minutes > 0 ? `${hours}h ${minutes}min` : `${hours}h`;
}

export function workspaceActivityTimestamp(
	activity: WorkspaceAgentActivity,
): string | null {
	return activity.state === "active"
		? activity.startedAt
		: activity.completedAt;
}
