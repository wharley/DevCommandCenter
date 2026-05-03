import { queryOptions } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { loadWorkspaceSessions } from "@/lib/session-api";
import type { RuntimeSessionSnapshot } from "./workbench-types";

type SessionSnapshotSource = Pick<
	WorkspaceSessionSummary,
	"session" | "projection" | "lastTurnPrompt" | "lastTurnState"
>;

export function workspaceSessionsQueryOptions(workspaceId: string | null) {
	return queryOptions<WorkspaceSessionSummary[]>({
		queryKey: workspaceId
			? dccQueryKeys.workspaceSessions(workspaceId)
			: dccQueryKeys.workspaceSessions("__none__"),
		queryFn: async () => {
			if (!workspaceId) {
				return [];
			}

			return loadWorkspaceSessions(workspaceId);
		},
		enabled: Boolean(workspaceId),
		staleTime: 0,
	});
}

export function workspaceSessionSnapshotFromSummary(
	summary: SessionSnapshotSource,
): RuntimeSessionSnapshot {
	return {
		sessionId: summary.session.id,
		projectId: summary.session.projectId,
		workspaceId: summary.session.workspaceId,
		providerId: summary.session.providerId,
		model: summary.session.model,
		state: summary.projection.state,
		turnCount: summary.projection.turnCount,
		checkpointCount: summary.projection.checkpointCount,
		activeTurnId: summary.projection.activeTurnId ?? null,
		lastTurnPrompt: summary.lastTurnPrompt ?? null,
		lastTurnState: summary.lastTurnState ?? null,
	};
}
