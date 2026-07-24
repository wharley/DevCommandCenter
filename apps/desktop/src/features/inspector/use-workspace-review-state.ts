import { useQuery } from "@tanstack/react-query";
import { workspaceReviewState } from "@/lib/workspace-api";

export const WORKSPACE_REVIEW_STATE_QUERY_KEY = "workspaceReviewState";
const WORKSPACE_REVIEW_STATE_REFETCH_INTERVAL_MS = 15_000;

export function useWorkspaceReviewState(
	workspaceRoot: string | null,
	branch: string | null,
	forgeLogin: string | null = null,
	enabled = true,
) {
	const root = workspaceRoot?.trim() ?? "";
	const currentBranch = branch?.trim() ?? "";
	const selectedLogin = forgeLogin?.trim() ?? "";

	return useQuery({
		queryKey: [
			WORKSPACE_REVIEW_STATE_QUERY_KEY,
			root,
			currentBranch,
			selectedLogin,
		],
		queryFn: () =>
			workspaceReviewState({
				workspaceRoot: root,
				branch: currentBranch || null,
				forgeLogin: selectedLogin || null,
			}),
		enabled: Boolean(enabled && root),
		staleTime: 10_000,
		refetchOnWindowFocus: true,
		refetchInterval: WORKSPACE_REVIEW_STATE_REFETCH_INTERVAL_MS,
	});
}
