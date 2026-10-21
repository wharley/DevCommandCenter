import { useQuery } from "@tanstack/react-query";
import type { Delegation } from "@dcc/contracts";
import { listDelegations } from "@/lib/delegation-api";
import {
	childTasksByWorkspace,
	delegatedSessionIds,
	type ChildTaskEntry,
} from "./sidebar-child-tasks.logic";

const EMPTY_TASKS = new Map<string, ChildTaskEntry[]>();
const EMPTY_IDS: ReadonlySet<string> = new Set();

/** One poll of every delegation, shared by the sidebar's readers below. */
function useAllDelegations<T>(enabled: boolean, select: (delegations: Delegation[]) => T) {
	return useQuery({
		queryKey: ["delegations", "all"],
		queryFn: async () => (await listDelegations({ workspaceId: null, parentSessionId: null })).delegations,
		select,
		enabled,
		staleTime: 5_000,
		refetchInterval: 10_000,
	});
}

/** Every workspace's tasks still in play, for the sidebar's nested rows. */
export function useChildTasksByWorkspace(enabled: boolean): Map<string, ChildTaskEntry[]> {
	return useAllDelegations(enabled, childTasksByWorkspace).data ?? EMPTY_TASKS;
}

/** The conversations an agent delegated to: none of them stands for its task. */
export function useDelegatedSessionIds(enabled: boolean): ReadonlySet<string> {
	return useAllDelegations(enabled, delegatedSessionIds).data ?? EMPTY_IDS;
}
