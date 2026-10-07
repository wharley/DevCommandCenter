import { useQuery } from "@tanstack/react-query";
import type { ModelTokenUsage } from "@dcc/contracts";
import { useMemo } from "react";
import { DCC_QUERY_GC_TIME_MS, dccQueryKeys } from "@/lib/query-client";
import { loadSessionTurnUsage } from "@/lib/usage-api";

const EMPTY = new Map<string, ModelTokenUsage[]>();

/**
 * Exact provider-reported usage per turn, for the turn footer. Refreshed with
 * the thread history when a turn settles; never estimated client-side.
 */
export function useSessionTurnUsage(sessionId: string | null | undefined) {
	const query = useQuery({
		queryKey: dccQueryKeys.sessionTurnUsage(sessionId ?? "__none__"),
		queryFn: () => loadSessionTurnUsage(sessionId as string),
		enabled: Boolean(sessionId),
		staleTime: 30_000,
		gcTime: DCC_QUERY_GC_TIME_MS.history,
	});
	return useMemo(() => {
		if (!query.data?.length) return EMPTY;
		return new Map(query.data.map((turn) => [turn.turnId, turn.models]));
	}, [query.data]);
}
