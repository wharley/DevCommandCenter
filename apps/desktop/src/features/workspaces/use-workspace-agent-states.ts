import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import type { WorkspaceSummary } from "./types";

export type AgentState = "active" | "completed" | "aborted";

export type WorkspaceAgentActivity = {
	state: AgentState;
	startedAt: string | null;
	completedAt: string | null;
};

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
	const running = summaries.find(isRunningSession);
	if (running) {
		return {
			state: "active",
			startedAt:
				running.lastTurnStartedAt ??
				running.projection.updatedAt ??
				running.session.updatedAt,
			completedAt: null,
		};
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

export function deriveAgentStateFromSessions(
	summaries: WorkspaceSessionSummary[],
): AgentState | null {
	return deriveAgentActivityFromSessions(summaries)?.state ?? null;
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
			const query = sessionQueries[index];
			const activity = deriveAgentActivityFromSessions(query?.data ?? []);
			if (activity) {
				activities[workspace.id] = activity;
			}
			return activities;
		},
		{},
	);
}
