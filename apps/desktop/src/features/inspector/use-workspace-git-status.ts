import { useQuery } from "@tanstack/react-query";
import { workspaceGitStatus } from "@/lib/workspace-api";

export const WORKSPACE_GIT_STATUS_QUERY_KEY = "workspaceGitStatus";

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
				};
			}
			return workspaceGitStatus({ workspaceRoot: root });
		},
		enabled: Boolean(root),
		staleTime: 8_000,
		refetchOnWindowFocus: true,
	});
}
