import { queryOptions } from "@tanstack/react-query";
import type { SessionEventRecord } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { loadSessionThreadEvents } from "@/lib/session-api";

export function sessionThreadHistoryQueryOptions(sessionId: string | null) {
	return queryOptions<SessionEventRecord[]>({
		queryKey: sessionId
			? dccQueryKeys.sessionThreads(sessionId)
			: dccQueryKeys.sessionThreads("__none__"),
		queryFn: async () => {
			if (!sessionId) {
				return [];
			}

			return loadSessionThreadEvents(sessionId);
		},
		enabled: Boolean(sessionId),
		staleTime: 0,
	});
}
