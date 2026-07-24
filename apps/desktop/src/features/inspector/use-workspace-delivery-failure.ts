import { useQuery } from "@tanstack/react-query";
import { workspaceDeliveryFailureSnapshot } from "@/lib/workspace-api";

export const WORKSPACE_DELIVERY_FAILURE_QUERY_KEY =
	"workspaceDeliveryFailureSnapshot";
const DELIVERY_FAILURE_REFETCH_INTERVAL_MS = 15_000;

export function useWorkspaceDeliveryFailure(
	workspaceRoot: string | null,
	branch: string | null,
	enabled = true,
) {
	const root = workspaceRoot?.trim() ?? "";
	const currentBranch = branch?.trim() ?? "";

	return useQuery({
		queryKey: [
			WORKSPACE_DELIVERY_FAILURE_QUERY_KEY,
			root,
			currentBranch,
		],
		queryFn: () =>
			workspaceDeliveryFailureSnapshot({
				workspaceRoot: root,
			}),
		enabled: Boolean(enabled && root),
		staleTime: 10_000,
		refetchOnWindowFocus: true,
		refetchInterval: DELIVERY_FAILURE_REFETCH_INTERVAL_MS,
	});
}
