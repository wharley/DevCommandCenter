import { useQuery } from "@tanstack/react-query";
import { workspaceGitStatus } from "@/lib/workspace-api";

export const WORKSPACE_GIT_STATUS_QUERY_KEY = "workspaceGitStatus";
const GIT_INSPECTOR_REFETCH_INTERVAL_MS = 10_000;

export function useWorkspaceGitStatus(workspaceRoot: string | null) {
	const root = workspaceRoot?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
		queryFn: async () => {
			if (!root) {
				return {
					staged: [],
					unstaged: [],
					currentBranch: null,
					aheadOfRemoteCount: 0,
					behindOfRemoteCount: 0,
					conflictCount: 0,
					mergeInProgress: false,
				};
			}
			return workspaceGitStatus({ workspaceRoot: root });
		},
		enabled: Boolean(root),
		staleTime: 8_000,
		refetchOnWindowFocus: true,
		refetchInterval: GIT_INSPECTOR_REFETCH_INTERVAL_MS,
	});
}
