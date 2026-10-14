import { useQuery } from "@tanstack/react-query";
import { listDelegations } from "@/lib/delegation-api";
import { childTasksByWorkspace, type ChildTaskEntry } from "./sidebar-child-tasks.logic";

const EMPTY = new Map<string, ChildTaskEntry[]>();

/** Every workspace's tasks still in play, for the sidebar's nested rows. */
export function useChildTasksByWorkspace(enabled: boolean): Map<string, ChildTaskEntry[]> {
	const query = useQuery({
		queryKey: ["delegations", "child-tasks"],
		queryFn: async () => {
			const output = await listDelegations({ workspaceId: null, parentSessionId: null });
			return childTasksByWorkspace(output.delegations);
		},
		enabled,
		staleTime: 5_000,
		refetchInterval: 10_000,
	});
	return query.data ?? EMPTY;
}
