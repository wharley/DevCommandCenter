import { queryOptions } from "@tanstack/react-query";
import type { SessionEventRecord } from "@dcc/contracts";
import { dccQueryKeys } from "@/lib/query-client";
import { loadSessionThreadEvents } from "@/lib/session-api";

export function sessionThreadHistoryQueryOptions(
	sessionId: string | null,
	input?: { scope?: string; enabled?: boolean; refetchInterval?: number | false },
) {
	const scope = input?.scope ?? "local";
	const isEnabled = input?.enabled ?? true;
	const refetchInterval = input?.refetchInterval;
	return queryOptions<SessionEventRecord[]>({
		queryKey: sessionId
			? dccQueryKeys.sessionThreads(sessionId, scope)
			: dccQueryKeys.sessionThreads("__none__", scope),
		queryFn: async () => {
			if (!sessionId) {
				return [];
			}

			return loadSessionThreadEvents(sessionId);
		},
		enabled: isEnabled && Boolean(sessionId),
		staleTime: 0,
		refetchInterval,
	});
}
