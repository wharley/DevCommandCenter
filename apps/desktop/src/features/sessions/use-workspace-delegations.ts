import { useQuery } from "@tanstack/react-query";
import type { Delegation } from "@dcc/contracts";
import { listDelegations } from "@/lib/delegation-api";

/**
 * Every delegation of a workspace. The delegation card, the Inspector and the
 * lineage menu share this cache entry, so the list is polled once.
 */
export function useWorkspaceDelegations(workspaceId: string | null) {
	return useQuery({
		queryKey: ["delegations", workspaceId],
		queryFn: async () => {
			if (!workspaceId) {
				return [] as Delegation[];
			}
			const output = await listDelegations({ workspaceId, parentSessionId: null });
			return output.delegations;
		},
		enabled: Boolean(workspaceId),
		staleTime: 5_000,
		refetchInterval: 10_000,
	});
}
