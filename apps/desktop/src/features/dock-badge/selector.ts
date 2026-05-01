import type { WorkspaceSummary } from "@/features/workspaces";

export function selectUnreadSessionCount(
	workspaces: WorkspaceSummary[],
) {
	return workspaces.reduce((count, workspace) => {
		return count + (workspace.unreadSessionCount ?? 0);
	}, 0);
}
