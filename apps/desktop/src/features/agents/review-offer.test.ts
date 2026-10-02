import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { callAgentBlockKey, hasReviewableChanges, reviewOfferKey } from "./review-offer";
import type { AgentView } from "./use-agents";

function summary(id: string, lastTurnState: string | null, completedAt: string | null) {
	return {
		session: { id },
		projection: { state: "active", activeTurnId: lastTurnState === "running" ? "turn" : null },
		lastTurnState,
		lastTurnCompletedAt: completedAt,
	} as unknown as WorkspaceSessionSummary;
}

function reviewer(sessions: Array<{ workspaceId: string; updatedAt: string }> = []) {
	return {
		id: "reviewer",
		preset: "reviewer",
		offerPrompt: "Posso revisar?",
		sessions,
	} as unknown as AgentView;
}

const base = {
	agent: reviewer(),
	workspaceId: "ws",
	sessionId: "main",
	sessions: [summary("main", "completed", "2026-10-01T10:00:00.000Z")],
	changedFileCount: 2,
	isAgentSession: false,
};

describe("calling an agent", () => {
	it("needs something to review only for the built-in reviewer", () => {
		const empty = { staged: [], unstaged: [], aheadOfRemoteCount: 0 };
		expect(hasReviewableChanges(undefined)).toBeUndefined();
		expect(hasReviewableChanges(empty)).toBe(false);
		expect(hasReviewableChanges({ ...empty, unstaged: [{}] })).toBe(true);
		expect(hasReviewableChanges({ ...empty, aheadOfRemoteCount: 2 })).toBe(true);

		const reviewer = { preset: "reviewer", busy: false };
		expect(callAgentBlockKey({ ...reviewer, hasChanges: false })).toBe("agents.call.nothingToReview");
		expect(callAgentBlockKey({ ...reviewer, hasChanges: true })).toBeNull();
		// Unknown is not empty: do not block while git status is loading.
		expect(callAgentBlockKey({ ...reviewer, hasChanges: undefined })).toBeNull();
		expect(callAgentBlockKey({ preset: null, busy: false, hasChanges: false })).toBeNull();
		expect(callAgentBlockKey({ ...reviewer, busy: true, hasChanges: true })).toBe("agents.call.busy");
	});
});

describe("review offer", () => {
	it("offers once per finished turn that left changes", () => {
		expect(reviewOfferKey(base)).toBe("reviewer:main:2026-10-01T10:00:00.000Z");
	});

	it("stays quiet without a reviewer, changes, or a finished turn", () => {
		expect(reviewOfferKey({ ...base, agent: undefined })).toBeNull();
		// An agent the person wrote never offers itself, whatever its text says.
		expect(
			reviewOfferKey({ ...base, agent: { ...reviewer(), preset: null } as AgentView }),
		).toBeNull();
		// The person turned the built-in offer off.
		expect(
			reviewOfferKey({ ...base, agent: { ...reviewer(), offerPrompt: " " } as AgentView }),
		).toBeNull();
		expect(reviewOfferKey({ ...base, changedFileCount: 0 })).toBeNull();
		expect(reviewOfferKey({ ...base, sessionId: null })).toBeNull();
		expect(reviewOfferKey({ ...base, isAgentSession: true })).toBeNull();
		expect(
			reviewOfferKey({ ...base, sessions: [summary("main", "aborted", "2026-10-01T10:00:00.000Z")] }),
		).toBeNull();
	});

	it("waits while anything in the task is still running", () => {
		expect(
			reviewOfferKey({
				...base,
				sessions: [...base.sessions, summary("other", "running", null)],
			}),
		).toBeNull();
	});

	it("does not offer again after the reviewer ran for that turn", () => {
		expect(
			reviewOfferKey({
				...base,
				agent: reviewer([{ workspaceId: "ws", updatedAt: "2026-10-01T10:05:00.000Z" }]),
			}),
		).toBeNull();
		// A review of another task, or one older than the turn, does not count.
		expect(
			reviewOfferKey({
				...base,
				agent: reviewer([
					{ workspaceId: "other", updatedAt: "2026-10-01T10:05:00.000Z" },
					{ workspaceId: "ws", updatedAt: "2026-10-01T09:00:00.000Z" },
				]),
			}),
		).not.toBeNull();
	});
});
