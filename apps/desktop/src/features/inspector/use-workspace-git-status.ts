import { useQuery } from "@tanstack/react-query";
import { workspaceGitStatus } from "@/lib/workspace-api";

export const WORKSPACE_GIT_STATUS_QUERY_KEY = "workspaceGitStatus";
const GIT_INSPECTOR_REFETCH_INTERVAL_MS = 10_000;

type WorkspaceGitStatusQueryOptions = {
	staleTime?: number;
	refetchInterval?: number | false;
};

export function useWorkspaceGitStatus(
	workspaceRoot: string | null,
	options?: WorkspaceGitStatusQueryOptions,
) {
	const root = workspaceRoot?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
		queryFn: async () => {
			if (!root) {
				return {
					staged: [],
					unstaged: [],
					stagedFingerprint: "",
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
		staleTime: options?.staleTime ?? 8_000,
		refetchOnWindowFocus: true,
		refetchInterval:
			options?.refetchInterval ?? GIT_INSPECTOR_REFETCH_INTERVAL_MS,
	});
}
