import { beforeEach, describe, expect, it } from "vitest";
import {
	emptyFeedbackDraft,
	feedbackStatus,
	readFeedbackDraft,
	readFeedbackHistory,
	saveFeedbackDraft,
	saveFeedbackHistory,
	type FeedbackIssue,
} from "./feedback-api";

const issue: FeedbackIssue = {
	number: 42,
	title: "Bug",
	category: "bug",
	url: "",
	createdAt: "2026-09-11T12:00:00Z",
	state: "closed",
	stateReason: null,
};
beforeEach(() => localStorage.clear());

describe("feedback persistence and status", () => {
	it("does not equate every closed report with a completed fix", () => {
		expect(feedbackStatus(issue)).toBe("closed");
		expect(feedbackStatus({ ...issue, stateReason: "not_planned" })).toBe(
			"notPlanned",
		);
		expect(feedbackStatus({ ...issue, stateReason: "completed" })).toBe(
			"completed",
		);
		expect(
			feedbackStatus({ ...issue, state: "open", stateReason: "reopened" }),
		).toBe("open");
	});
	it("restores the exact request and account after an uncertain submission", () => {
		const draft = {
			...emptyFeedbackDraft(),
			title: "Issue",
			description: "Details",
		};
		const { pending: _, ...fields } = draft;
		draft.pending = { ...fields, login: "alice" };
		expect(saveFeedbackDraft(draft)).toBe(true);
		expect(readFeedbackDraft()).toEqual(draft);
	});
	it("keeps offline history isolated by account and page", () => {
		saveFeedbackHistory("alice", 1, { issues: [issue], hasNext: true });
		expect(readFeedbackHistory("ALICE", 1)?.issues[0].number).toBe(42);
		expect(readFeedbackHistory("bob", 1)).toBeUndefined();
		expect(readFeedbackHistory("alice", 2)).toBeUndefined();
	});
	it("recovers from malformed saved data", () => {
		localStorage.setItem("dcc.feedback.draft.v1", "{bad");
		localStorage.setItem(
			"dcc.feedback.history.v1.alice.1",
			'{"issues":[{}],"hasNext":false}',
		);
		expect(readFeedbackDraft().description).toBe("");
		expect(readFeedbackHistory("alice", 1)).toBeUndefined();
	});
});
