import { queryOptions } from "@tanstack/react-query";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { loadWorkspaceSessions } from "@/lib/session-api";
import type { RuntimeSessionSnapshot } from "./workbench-types";

type SessionSnapshotSource = Pick<
	WorkspaceSessionSummary,
	"session" | "projection" | "lastTurnPrompt" | "lastTurnState"
>;

export function workspaceSessionsQueryOptions(
	workspaceId: string | null,
	input?: { enabled?: boolean; scope?: string; refetchInterval?: number | false },
) {
	const scope = input?.scope ?? "local";
	const isEnabled = input?.enabled ?? true;
	const refetchInterval = input?.refetchInterval;
	return queryOptions<WorkspaceSessionSummary[]>({
		queryKey: workspaceId
			? dccQueryKeys.workspaceSessions(workspaceId, scope)
			: dccQueryKeys.workspaceSessions("__none__", scope),
		queryFn: async () => {
			if (!workspaceId) {
				return [];
			}

			return loadWorkspaceSessions(workspaceId);
		},
		enabled: isEnabled && Boolean(workspaceId),
		staleTime: 0,
		refetchInterval,
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
