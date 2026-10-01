import { describe, expect, it } from "vitest";
import { isLatestExecution, isReviewCurrent } from "./use-reviewer-findings";

describe("reviewer findings freshness", () => {
	it("drops a review once another agent finishes a turn after it", () => {
		const reviewedAt = "2026-10-01T10:05:00.000Z";
		expect(isReviewCurrent(reviewedAt, [])).toBe(true);
		expect(isReviewCurrent(reviewedAt, ["2026-10-01T10:00:00.000Z", null, undefined])).toBe(true);
		expect(isReviewCurrent(reviewedAt, ["2026-10-01T10:06:00.000Z"])).toBe(false);
	});

	it("marks only the latest execution of a session", () => {
		const last = "2026-10-01T10:00:00.000Z";
		expect(isLatestExecution("2026-10-01T10:00:00.400Z", last)).toBe(true);
		expect(isLatestExecution("2026-10-01T09:59:58.000Z", last)).toBe(true);
		expect(isLatestExecution("2026-10-01T09:30:00.000Z", last)).toBe(false);
		expect(isLatestExecution(null, last)).toBe(false);
		expect(isLatestExecution(last, undefined)).toBe(false);
	});
});
