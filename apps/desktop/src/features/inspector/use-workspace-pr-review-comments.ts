import { useQuery } from "@tanstack/react-query";
import { workspacePrReviewComments } from "@/lib/workspace-api";

export const WORKSPACE_PR_REVIEW_COMMENTS_QUERY_KEY =
	"workspacePrReviewComments";
const WORKSPACE_PR_REVIEW_COMMENTS_REFETCH_INTERVAL_MS = 30_000;

export function useWorkspacePrReviewComments(
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
			WORKSPACE_PR_REVIEW_COMMENTS_QUERY_KEY,
			root,
			currentBranch,
			selectedLogin,
		],
		queryFn: () =>
			workspacePrReviewComments({
				workspaceRoot: root,
				branch: currentBranch.length > 0 ? currentBranch : null,
				forgeLogin: selectedLogin.length > 0 ? selectedLogin : null,
			}),
		enabled: Boolean(enabled && root),
		staleTime: 20_000,
		refetchOnWindowFocus: true,
		refetchInterval: WORKSPACE_PR_REVIEW_COMMENTS_REFETCH_INTERVAL_MS,
	});
}
