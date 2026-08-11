import { useQuery } from "@tanstack/react-query";
import { workspacePrStatus } from "@/lib/workspace-api";

export const WORKSPACE_PR_STATUS_QUERY_KEY = "workspacePrStatus";
const WORKSPACE_PR_REFETCH_INTERVAL_MS = 10_000;

type WorkspacePrStatusQueryOptions = {
	staleTime?: number;
	refetchInterval?: number | false;
};

export function useWorkspacePrStatus(
	workspaceRoot: string | null,
	branch: string | null,
	forgeLogin: string | null = null,
	options?: WorkspacePrStatusQueryOptions,
) {
	const root = workspaceRoot?.trim() ?? "";
	const currentBranch = branch?.trim() ?? "";
	const selectedLogin = forgeLogin?.trim() ?? "";

	return useQuery({
		queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root, currentBranch, selectedLogin],
		queryFn: async () => {
			if (!root) {
				return {
					provider: null,
					host: null,
					number: null,
					title: null,
					url: null,
					headBranch: null,
					baseBranch: null,
					state: null,
					isDraft: false,
					mergeable: null,
					mergeStateStatus: null,
				};
			}

			return workspacePrStatus({
				workspaceRoot: root,
				branch: currentBranch.length > 0 ? currentBranch : null,
				forgeLogin: selectedLogin.length > 0 ? selectedLogin : null,
			});
		},
		enabled: Boolean(root),
		staleTime: options?.staleTime ?? 8_000,
		refetchOnWindowFocus: true,
		refetchInterval:
			options?.refetchInterval ?? WORKSPACE_PR_REFETCH_INTERVAL_MS,
	});
}
