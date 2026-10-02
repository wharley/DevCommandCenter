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
 * The findings of the latest review a reviewer session holds, or `null` when
 * it has none yet. A later answer without the review block (the person asked
 * the reviewer a question) does not replace the review before it.
 */
export async function loadLatestReview(sessionId: string): Promise<ReviewFinding[] | null> {
	const events = await loadSessionThreadEvents(sessionId);
	const messages = projectWorkspaceMessages(events, [], sessionId);
	for (let index = messages.length - 1; index >= 0; index -= 1) {
		const message = messages[index]!;
		if (message.role !== "assistant" || message.assistantPhase === "commentary") {
			continue;
		}
		const findings = parseReviewFindings(message.content);
		if (findings) {
			return findings;
		}
	}
	return null;
}

/**
 * Findings of the latest finished review by the built-in reviewer in a task.
 * Empty once the task changed after the review, so the diff never shows
 * markers on lines that have moved.
 */
export function useReviewerFindings(workspaceId: string | null, scope = "local"): ReviewFinding[] {
	const { agents, agentBySessionId } = useAgents();
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
		queryFn: async () => (await loadLatestReview(sessionId as string)) ?? NO_FINDINGS,
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

/** Clock skew allowed between a turn's snapshot and its session summary. */
const SAME_TURN_TOLERANCE_MS = 5_000;

/** Whether an execution is the most recent one its session completed. */
export function isLatestExecution(
	executionCompletedAt: string | null,
	sessionLastTurnCompletedAt: string | null | undefined,
): boolean {
	if (!executionCompletedAt || !sessionLastTurnCompletedAt) {
		return false;
	}
	return (
		Date.parse(sessionLastTurnCompletedAt) - Date.parse(executionCompletedAt) <=
		SAME_TURN_TOLERANCE_MS
	);
}

/**
 * Findings to mark on the patch of one execution: the task's current findings
 * when that execution is the latest of its session, nothing otherwise.
 */
export function useReviewerFindingsForTurn(
	execution: { workspaceId: string; sessionId: string; completedAt: string | null },
	scope = "local",
): ReviewFinding[] {
	const findings = useReviewerFindings(execution.workspaceId, scope);
	const sessions = useQuery(workspaceSessionsQueryOptions(execution.workspaceId, { scope })).data;
	const lastCompletedAt = sessions?.find(
		(summary) => summary.session.id === execution.sessionId,
	)?.lastTurnCompletedAt;
	return isLatestExecution(execution.completedAt, lastCompletedAt) ? findings : NO_FINDINGS;
}
