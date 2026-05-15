import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import type { WorkspaceSummary } from "./types";

export type AgentState = "active" | "completed" | "aborted";

function isRunningSession(summary: WorkspaceSessionSummary): boolean {
	return (
		summary.lastTurnState === "running" ||
		(summary.projection.state === "active" &&
			summary.projection.activeTurnId != null &&
			summary.projection.activeTurnId.length > 0)
	);
}

export function deriveAgentStateFromSessions(
	summaries: WorkspaceSessionSummary[],
): AgentState | null {
	if (summaries.some(isRunningSession)) {
		return "active";
	}

	const latest = summaries[0];
	if (!latest) {
		return null;
	}

	if (latest.lastTurnState === "aborted" || latest.projection.state === "aborted") {
		return "aborted";
	}

	if (
		latest.lastTurnState === "completed" ||
		latest.projection.state === "completed" ||
		latest.projection.turnCount > 0
	) {
		return "completed";
	}

	return null;
}

export function useWorkspaceAgentStates(
	workspaces: Pick<WorkspaceSummary, "id" | "status">[],
	input?: { enabled?: boolean; scope?: string },
): Record<string, AgentState> {
	const isEnabled = input?.enabled ?? true;
	const scope = input?.scope ?? "local";
	const trackedWorkspaces = useMemo(
		() =>
			isEnabled
				? workspaces.filter((workspace) => workspace.status !== "archived")
				: [],
		[isEnabled, workspaces],
	);
	const sessionQueries = useQueries({
		queries: trackedWorkspaces.map((workspace) =>
			workspaceSessionsQueryOptions(workspace.id, { enabled: isEnabled, scope }),
		),
	});

	return trackedWorkspaces.reduce<Record<string, AgentState>>((states, workspace, index) => {
		const query = sessionQueries[index];
		const nextState = deriveAgentStateFromSessions(query?.data ?? []);
		if (nextState) {
			states[workspace.id] = nextState;
		}
		return states;
	}, {});
}
