import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { projectWorkspaceMessages } from "@/features/sessions/session-thread-history.logic";
import { workspaceSessionsQueryOptions } from "@/features/sessions/workspace-sessions-query";
import { loadSessionThreadEvents } from "@/lib/session-api";
import { type ReviewFinding, parseReviewFindings } from "./review-findings";
import { useAgents } from "./use-agents";

const NO_FINDINGS: ReviewFinding[] = [];

/**
 * Whether a review still describes the task: nothing but agent sessions may
 * have finished a turn after it, or its line numbers no longer match.
 */
export function isReviewCurrent(
	reviewedAt: string,
	otherTurnsCompletedAt: Array<string | null | undefined>,
): boolean {
	return otherTurnsCompletedAt.every((completedAt) => !completedAt || completedAt <= reviewedAt);
}

/**
 * Findings of the latest finished review by the built-in reviewer in a task.
 * Empty once the task changed after the review, so the diff never shows
 * markers on lines that have moved.
 */
export function useReviewerFindings(workspaceId: string | null, scope = "local"): ReviewFinding[] {
	const { agents, agentBySessionId } = useAgents(scope);
	const reviewSession = agents
		.find((agent) => agent.preset === "reviewer")
		?.sessions.find((session) => session.workspaceId === workspaceId && session.state === "done");
	const sessionId = reviewSession?.sessionId ?? null;
	const sessions = useQuery(workspaceSessionsQueryOptions(workspaceId, { scope })).data;
	const completedAt = sessions?.find((summary) => summary.session.id === sessionId)
		?.lastTurnCompletedAt;

	const review = useQuery({
		// A new turn in the review session is a new review.
		queryKey: ["agentReview", sessionId, completedAt ?? null],
		enabled: Boolean(sessionId),
		staleTime: Number.POSITIVE_INFINITY,
		queryFn: async () => {
			const events = await loadSessionThreadEvents(sessionId as string);
			const last = projectWorkspaceMessages(events, [], sessionId)
				.filter(
					(message) =>
						message.role === "assistant" &&
						message.assistantPhase !== "commentary" &&
						message.content.trim().length > 0,
				)
				.at(-1);
			return last ? (parseReviewFindings(last.content) ?? NO_FINDINGS) : NO_FINDINGS;
		},
	});

	return useMemo(() => {
		if (!review.data?.length || !completedAt) {
			return NO_FINDINGS;
		}
		const otherTurns = (sessions ?? [])
			.filter((summary) => !agentBySessionId.has(summary.session.id))
			.map((summary) => summary.lastTurnCompletedAt);
		return isReviewCurrent(completedAt, otherTurns) ? review.data : NO_FINDINGS;
	}, [agentBySessionId, completedAt, review.data, sessions]);
}
