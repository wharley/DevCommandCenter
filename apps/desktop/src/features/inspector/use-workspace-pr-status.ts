import { useQuery } from "@tanstack/react-query";
import { workspacePrStatus } from "@/lib/workspace-api";

export const WORKSPACE_PR_STATUS_QUERY_KEY = "workspacePrStatus";

export function useWorkspacePrStatus(workspaceRoot: string | null, branch: string | null) {
	const root = workspaceRoot?.trim() ?? "";
	const currentBranch = branch?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root, currentBranch],
		queryFn: async () => {
			if (!root) {
				return {
					provider: null,
					number: null,
					title: null,
					url: null,
					headBranch: null,
					baseBranch: null,
					state: null,
					mergeable: null,
					mergeStateStatus: null,
				};
			}

			return workspacePrStatus({
				workspaceRoot: root,
				branch: currentBranch.length > 0 ? currentBranch : null,
			});
		},
		enabled: Boolean(root),
		staleTime: 8_000,
		refetchOnWindowFocus: true,
	});
}
