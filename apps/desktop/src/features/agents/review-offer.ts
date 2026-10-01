import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type { AgentView } from "./use-agents";

export function isSessionRunning(summary: WorkspaceSessionSummary): boolean {
	return (
		summary.lastTurnState === "running" ||
		(summary.projection.state === "active" && Boolean(summary.projection.activeTurnId))
	);
}

type GitChanges = {
	staged: unknown[];
	unstaged: unknown[];
	aheadOfRemoteCount: number;
};

/**
 * Whether the task has anything to review: local changes or commits not yet
 * pushed. `undefined` (status still loading) counts as unknown, not as empty.
 */
export function hasReviewableChanges(status: GitChanges | undefined): boolean | undefined {
	return status
		? status.staged.length + status.unstaged.length > 0 || status.aheadOfRemoteCount > 0
		: undefined;
}

/**
 * Why an agent cannot be called in the task right now, as a translation key.
 * The built-in reviewer needs something to review; a turn in progress blocks
 * every agent.
 */
export function callAgentBlockKey(input: {
	preset: string | null;
	busy: boolean;
	hasChanges: boolean | undefined;
}): "agents.call.busy" | "agents.call.nothingToReview" | null {
	if (input.busy) {
		return "agents.call.busy";
	}
	return input.preset === "reviewer" && input.hasChanges === false
		? "agents.call.nothingToReview"
		: null;
}

/**
 * Whether an agent should offer itself after a turn. Only agents with an
 * offer text do. It offers once per finished turn of a plain session, only
 * while the task has local changes and nothing is running, and not when the
 * agent already ran in the task after that turn.
 * Returns the key that identifies this offer, so a dismissal can be remembered.
 */
export function reviewOfferKey(input: {
	agent: AgentView | undefined;
	workspaceId: string;
	sessionId: string | null;
	sessions: WorkspaceSessionSummary[];
	changedFileCount: number;
	isAgentSession: boolean;
}): string | null {
	const { agent, workspaceId, sessionId, sessions, changedFileCount, isAgentSession } = input;
	if (!agent?.offerPrompt.trim() || !sessionId || isAgentSession || changedFileCount === 0) {
		return null;
	}
	if (sessions.some(isSessionRunning)) {
		return null;
	}
	const summary = sessions.find((candidate) => candidate.session.id === sessionId);
	const completedAt = summary?.lastTurnCompletedAt;
	if (!summary || summary.lastTurnState !== "completed" || !completedAt) {
		return null;
	}
	const alreadyRan = agent.sessions.some(
		(session) => session.workspaceId === workspaceId && session.updatedAt >= completedAt,
	);
	return alreadyRan ? null : `${agent.id}:${sessionId}:${completedAt}`;
}
