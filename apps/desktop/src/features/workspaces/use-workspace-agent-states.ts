import { hasWoken, isSnoozed } from "./workspace-snooze";
import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import type { WorkspaceBlocker, WorkspaceSessionSummary } from "@dcc/contracts";
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

/** Why a task shows up under "Precisa de você", most urgent first. */
export type AttentionReason =
	| "permission"
	| "input"
	| "conflicts"
	| "prConflicts"
	| "checksFailing"
	| "delegatedReview"
	| "woke"
	| "setup"
	| "aborted"
	| "completed";

const ATTENTION_ORDER: Record<AttentionReason, number> = {
	permission: 0,
	input: 1,
	conflicts: 2,
	prConflicts: 3,
	checksFailing: 4,
	delegatedReview: 5,
	woke: 6,
	setup: 7,
	aborted: 8,
	completed: 9,
};

const BLOCKER_REASON: Record<WorkspaceBlocker["kind"], AttentionReason> = {
	conflicts: "conflicts",
	pr_conflicts: "prConflicts",
	checks_failing: "checksFailing",
	delegated_edits_review: "delegatedReview",
};

export type AttentionWorkspaceItem = {
	workspace: WorkspaceSummary;
	activity: WorkspaceAgentActivity | null;
	reason: AttentionReason;
	/** PR number for PR blockers, count for delegated reviews. */
	count?: number | null;
};

function attentionAtMs(item: AttentionWorkspaceItem): number {
	const at =
		item.activity?.completedAt ?? item.activity?.startedAt ?? item.workspace.updatedAt ?? null;
	const parsed = at ? Date.parse(at) : Number.NaN;
	return Number.isNaN(parsed) ? 0 : parsed;
}

/**
 * Tasks that need the person: an agent blocked on approval or a question, a
 * workspace whose setup needs a hand, or a finished/interrupted turn whose
 * result has not been opened yet. Running agents are deliberately absent —
 * they live in their project group until they come back to the person.
 */
export function attentionWorkspaceItems(
	workspaces: WorkspaceSummary[],
	activities: Record<string, WorkspaceAgentActivity>,
	isResultUnread: (workspaceId: string, completedAt: string | null) => boolean,
	blockers: ReadonlyMap<string, readonly WorkspaceBlocker[]> = new Map(),
	now: number = Date.now(),
): AttentionWorkspaceItem[] {
	return workspaces
		.flatMap((workspace): AttentionWorkspaceItem[] => {
			if (
				workspace.status === "archived" ||
				workspace.status === "completed" ||
				isSnoozed(workspace, now)
			) {
				return [];
			}
			const activity = activities[workspace.id] ?? null;
			if (activity?.state === "waiting") {
				return [
					{
						workspace,
						activity,
						reason: activity.waitingFor === "permission" ? "permission" : "input",
					},
				];
			}
			// Git and review blockers outrank setup and unread results; a
			// running agent is left alone until it comes back.
			const blocker = isAgentTurnOpen(activity)
				? undefined
				: [...(blockers.get(workspace.id) ?? [])].sort(
						(left, right) =>
							ATTENTION_ORDER[BLOCKER_REASON[left.kind]] -
							ATTENTION_ORDER[BLOCKER_REASON[right.kind]],
					)[0];
			if (blocker) {
				return [
					{
						workspace,
						activity,
						reason: BLOCKER_REASON[blocker.kind],
						count: blocker.count ?? null,
					},
				];
			}
			if (hasWoken(workspace, now) && !isAgentTurnOpen(activity)) {
				return [{ workspace, activity, reason: "woke" }];
			}
			if (workspace.status === "setup_pending" && !isAgentTurnOpen(activity)) {
				return [{ workspace, activity, reason: "setup" }];
			}
			if (
				(activity?.state === "aborted" || activity?.state === "completed") &&
				isResultUnread(workspace.id, activity.completedAt)
			) {
				return [{ workspace, activity, reason: activity.state }];
			}
			return [];
		})
		.sort(
			(left, right) =>
				ATTENTION_ORDER[left.reason] - ATTENTION_ORDER[right.reason] ||
				attentionAtMs(right) - attentionAtMs(left) ||
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
