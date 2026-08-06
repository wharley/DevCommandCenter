import type { QueryClient } from "@tanstack/react-query";

export type SessionBufferStat = { sessionId: string; events: number; bytes: number };

function approximateBytes(value: unknown) {
	try {
		return JSON.stringify(value).length * 2;
	} catch {
		return 0;
	}
}

export function frontendMemorySnapshot(
	queryClient: QueryClient,
	sessionBuffers: SessionBufferStat[] = [],
) {
	const queryRoots: Record<string, { count: number; bytes: number }> = {};
	for (const query of queryClient.getQueryCache().getAll()) {
		const root = String(query.queryKey[0] ?? "unknown");
		const current = queryRoots[root] ?? { count: 0, bytes: 0 };
		current.count += 1;
		current.bytes += approximateBytes(query.state.data);
		queryRoots[root] = current;
	}
	return {
		capturedAt: new Date().toISOString(),
		queries: {
			count: Object.values(queryRoots).reduce((sum, entry) => sum + entry.count, 0),
			bytes: Object.values(queryRoots).reduce((sum, entry) => sum + entry.bytes, 0),
			byRoot: queryRoots,
		},
		liveEvents: {
			sessions: sessionBuffers.length,
			events: sessionBuffers.reduce((sum, entry) => sum + entry.events, 0),
			bytes: sessionBuffers.reduce((sum, entry) => sum + entry.bytes, 0),
			bySession: sessionBuffers,
		},
	};
}
