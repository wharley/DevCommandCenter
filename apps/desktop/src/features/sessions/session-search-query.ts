import { queryOptions } from "@tanstack/react-query";
import type { SessionSearchResult } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { searchSessionHistory } from "@/lib/session-api";

export function sessionSearchQueryOptions(query: string | null) {
	return queryOptions<SessionSearchResult[]>({
		queryKey: dccQueryKeys.sessionSearch(query ?? "__closed__"),
		queryFn: async () => searchSessionHistory(query ?? "", 40),
		enabled: query !== null,
		staleTime: 15_000,
	});
}

