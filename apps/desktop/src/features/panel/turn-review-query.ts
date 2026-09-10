import { useQuery } from "@tanstack/react-query";
import type { TurnReviewSummary } from "@dcc/contracts";
import { loadLastTurnReview } from "@/lib/session-api";

export function lastTurnReviewQueryKey(sessionId: string, workspaceId: string) {
	return ["lastTurnReview", sessionId, workspaceId] as const;
}

export function visibleTurnReviewSummary(
	data: TurnReviewSummary | null | undefined,
	isFetching: boolean,
): TurnReviewSummary | null {
	return isFetching ? null : (data ?? null);
}

export function useCachedTurnReviewSummary(
	sessionId: string | null,
	workspaceId: string | null,
): TurnReviewSummary | null {
	const query = useQuery({
		queryKey: lastTurnReviewQueryKey(sessionId ?? "", workspaceId ?? ""),
		queryFn: () => loadLastTurnReview(sessionId!, workspaceId!),
		enabled: Boolean(sessionId && workspaceId),
		// The action card unmounts while a turn is active and remounts at the
		// terminal event. A stale query must refetch on that mount so the card
		// never carries the previous turn's summary.
		staleTime: 0,
		refetchOnMount: "always",
	});
	return visibleTurnReviewSummary(query.data, query.isFetching);
}

export type TurnReviewTarget = {
	sessionId: string;
	workspaceId: string;
	turnId: string;
	filePath?: string;
};
export type TurnReviewRequest = TurnReviewTarget & { nonce: number };

export function turnReviewQueryOptions(target: TurnReviewTarget) {
	return {
		queryKey: [
			"turnReview",
			target.sessionId,
			target.workspaceId,
			target.turnId,
		] as const,
		queryFn: () =>
			loadLastTurnReview(target.sessionId, target.workspaceId, target.turnId),
		// Result capture can finish just after the terminal event reaches the UI.
		refetchInterval: (query: {
			state: {
				data: TurnReviewSummary | null | undefined;
				dataUpdateCount: number;
				status: string;
			};
		}) =>
			query.state.status !== "error" &&
			(query.state.data?.state === "collecting" ||
				(query.state.data == null && query.state.dataUpdateCount < 5))
				? 1500
				: (false as const),
	};
}
