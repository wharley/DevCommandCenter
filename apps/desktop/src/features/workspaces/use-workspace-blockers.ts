import { useQuery } from "@tanstack/react-query";
import type { WorkspaceBlocker } from "@dcc/contracts";
import { workspaceAttentionBlockers } from "@/lib/workspace-api";

/**
 * Blockers for every active task in one backend call, so a conflict or a
 * failing check surfaces even in a collapsed project. Local backend only.
 */
export function useWorkspaceBlockers(enabled: boolean) {
	return useQuery({
		queryKey: ["workspace-blockers"],
		queryFn: async () => (await workspaceAttentionBlockers()).blockers,
		enabled,
		staleTime: 30_000,
		refetchInterval: 60_000,
	});
}

export function blockersByWorkspace(blockers: readonly WorkspaceBlocker[]) {
	const byWorkspace = new Map<string, WorkspaceBlocker[]>();
	for (const blocker of blockers) {
		byWorkspace.set(blocker.workspaceId, [...(byWorkspace.get(blocker.workspaceId) ?? []), blocker]);
	}
	return byWorkspace;
}
