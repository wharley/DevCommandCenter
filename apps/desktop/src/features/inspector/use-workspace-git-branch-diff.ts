import { useQuery } from "@tanstack/react-query";
import { workspaceGitBranchDiff } from "@/lib/workspace-api";

export const WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY = "workspaceGitBranchDiff";
const WORKSPACE_BRANCH_DIFF_REFETCH_INTERVAL_MS = 10_000;

export function useWorkspaceGitBranchDiff(workspaceRoot: string | null) {
	const root = workspaceRoot?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
		queryFn: async () => {
			if (!root) {
				return { changes: [], baseBranch: null };
			}
			return workspaceGitBranchDiff({ workspaceRoot: root });
		},
		enabled: Boolean(root),
		staleTime: 15_000,
		refetchOnWindowFocus: true,
		refetchInterval: WORKSPACE_BRANCH_DIFF_REFETCH_INTERVAL_MS,
	});
}
