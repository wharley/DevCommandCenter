import { queryOptions } from "@tanstack/react-query";
import type { SessionSearchResult } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { searchSessionHistory } from "@/lib/session-api";

export function sessionSearchQueryOptions(
	query: string | null,
	input?: { scope?: string; enabled?: boolean },
) {
	const scope = input?.scope ?? "local";
	const isEnabled = input?.enabled ?? true;
	return queryOptions<SessionSearchResult[]>({
		queryKey: dccQueryKeys.sessionSearch(query ?? "__closed__", scope),
		queryFn: async () => searchSessionHistory(query ?? "", 40),
		enabled: isEnabled && query !== null,
		staleTime: 15_000,
	});
}
