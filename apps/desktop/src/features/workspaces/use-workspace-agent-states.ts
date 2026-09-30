import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import type { WorkspaceSummary } from "./types";

/** `waiting` is a turn that is still open but blocked on the user. */
export type AgentState = "active" | "waiting" | "completed" | "aborted";

export type AgentWaitingReason = "permission" | "input";

export type WorkspaceAgentActivity = {
	state: AgentState;
	startedAt: string | null;
	completedAt: string | null;
	waitingFor?: AgentWaitingReason;
	/** See `lastInteractionAtFromSessions`; drives the rail's task order. */
	lastInteractionAt?: string | null;
};

/** The turn has not finished yet, whether the agent is working or waiting. */
export function isAgentTurnOpen(
	activity: WorkspaceAgentActivity | null | undefined,
): boolean {
	return activity?.state === "active" || activity?.state === "waiting";
}

function waitingReason(
	summary: WorkspaceSessionSummary,
): AgentWaitingReason | null {
	return summary.lastTurnAwaitingUser === "permission" ||
		summary.lastTurnAwaitingUser === "input"
		? summary.lastTurnAwaitingUser
		: null;
}

export type RunningWorkspaceActivity = {
	workspace: WorkspaceSummary;
	activity: WorkspaceAgentActivity;
};

function activityStartedAtMs(activity: WorkspaceAgentActivity): number {
	if (!activity.startedAt) return 0;
	const parsed = Date.parse(activity.startedAt);
	return Number.isNaN(parsed) ? 0 : parsed;
}

export function runningWorkspaceActivities(
	workspaces: WorkspaceSummary[],
	activities: Record<string, WorkspaceAgentActivity>,
): RunningWorkspaceActivity[] {
	return workspaces
		.flatMap((workspace) => {
			if (workspace.status === "archived" || workspace.status === "completed") {
				return [];
			}
			const activity = activities[workspace.id];
			return isAgentTurnOpen(activity) && activity
				? [{ workspace, activity }]
				: [];
		})
		.sort(
			(left, right) =>
				// Agents blocked on the user come first; they are the ones to act on.
				Number(right.activity.state === "waiting") -
					Number(left.activity.state === "waiting") ||
				activityStartedAtMs(right.activity) - activityStartedAtMs(left.activity) ||
				left.workspace.id.localeCompare(right.workspace.id),
		);
}

function isRunningSession(summary: WorkspaceSessionSummary): boolean {
	return (
		summary.lastTurnState === "running" ||
		(summary.projection.state === "active" &&
			summary.projection.activeTurnId != null &&
			summary.projection.activeTurnId.length > 0)
	);
}

export function deriveAgentActivityFromSessions(
	summaries: WorkspaceSessionSummary[],
): WorkspaceAgentActivity | null {
	const runningSessions = summaries.filter(isRunningSession);
	const waiting = runningSessions.find((summary) => waitingReason(summary));
	const running = waiting ?? runningSessions[0];
	if (running) {
		const startedAt =
			running.lastTurnStartedAt ??
			running.projection.updatedAt ??
			running.session.updatedAt;
		const reason = waitingReason(running);
		return reason
			? { state: "waiting", startedAt, completedAt: null, waitingFor: reason }
			: { state: "active", startedAt, completedAt: null };
	}

	const latest = summaries[0];
	if (!latest) {
		return null;
	}

	if (latest.lastTurnState === "aborted" || latest.projection.state === "aborted") {
		return {
			state: "aborted",
			startedAt: latest.lastTurnStartedAt,
			completedAt:
				latest.lastTurnCompletedAt ??
				latest.projection.updatedAt ??
				latest.session.updatedAt,
		};
	}

	if (
		latest.lastTurnState === "completed" ||
		latest.projection.state === "completed" ||
		latest.projection.turnCount > 0
	) {
		return {
			state: "completed",
			startedAt: latest.lastTurnStartedAt,
			completedAt:
				latest.lastTurnCompletedAt ??
				latest.projection.updatedAt ??
				latest.session.updatedAt,
		};
	}

	return null;
}

/**
 * When the user last acted on the task: the newest turn start across its
 * sessions. Agent progress (deltas, completion) deliberately does not count, so
 * the rail only reorders when the user does something, never under the cursor.
 */
export function lastInteractionAtFromSessions(
	summaries: WorkspaceSessionSummary[],
): string | null {
	let latest: string | null = null;
	let latestMs = Number.NEGATIVE_INFINITY;
	for (const summary of summaries) {
		const startedAt = summary.lastTurnStartedAt;
		const ms = startedAt ? Date.parse(startedAt) : Number.NaN;
		if (!Number.isNaN(ms) && ms > latestMs) {
			latest = startedAt;
			latestMs = ms;
		}
	}
	return latest;
}

export function deriveAgentStateFromSessions(
	summaries: WorkspaceSessionSummary[],
): AgentState | null {
	return deriveAgentActivityFromSessions(summaries)?.state ?? null;
}

/** The running provider wins; otherwise use the most recent workspace session. */
export function deriveProviderIdFromSessions(
	summaries: WorkspaceSessionSummary[],
): string | null {
	const summary = summaries.find(isRunningSession) ?? summaries[0];
	const providerId = summary?.session.providerId?.trim();
	return providerId || null;
}

export function useWorkspaceAgentActivities(
	workspaces: Pick<WorkspaceSummary, "id" | "status">[],
	input?: { enabled?: boolean; scope?: string },
): Record<string, WorkspaceAgentActivity> {
	const isEnabled = input?.enabled ?? true;
	const scope = input?.scope ?? "local";
	const trackedWorkspaces = useMemo(
		() =>
			isEnabled
				? workspaces.filter(
						(workspace) =>
							workspace.status !== "archived" &&
							workspace.status !== "completed",
					)
				: [],
		[isEnabled, workspaces],
	);
	const sessionQueries = useQueries({
		queries: trackedWorkspaces.map((workspace) =>
			workspaceSessionsQueryOptions(workspace.id, { enabled: isEnabled, scope }),
		),
	});

	return trackedWorkspaces.reduce<Record<string, WorkspaceAgentActivity>>(
		(activities, workspace, index) => {
			const summaries = sessionQueries[index]?.data ?? [];
			const activity = deriveAgentActivityFromSessions(summaries);
			if (activity) {
				activities[workspace.id] = {
					...activity,
					lastInteractionAt: lastInteractionAtFromSessions(summaries),
				};
			}
			return activities;
		},
		{},
	);
}

export function useWorkspaceProviderIds(
	workspaces: Pick<WorkspaceSummary, "id">[],
	input?: { enabled?: boolean; scope?: string },
): Record<string, string> {
	const isEnabled = input?.enabled ?? true;
	const scope = input?.scope ?? "local";
	const trackedWorkspaces = useMemo(
		() => (isEnabled ? workspaces : []),
		[isEnabled, workspaces],
	);
	const sessionQueries = useQueries({
		queries: trackedWorkspaces.map((workspace) =>
			workspaceSessionsQueryOptions(workspace.id, { enabled: isEnabled, scope }),
		),
	});

	return trackedWorkspaces.reduce<Record<string, string>>(
		(providerIds, workspace, index) => {
			const providerId = deriveProviderIdFromSessions(sessionQueries[index]?.data ?? []);
			if (providerId) {
				providerIds[workspace.id] = providerId;
			}
			return providerIds;
		},
		{},
	);
}
