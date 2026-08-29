import { describe, expect, it } from "vitest";
import {
	changeGroupBelongsToScope,
	defaultInspectorChangesScope,
	reviewCardDiffHeight,
	shouldEagerLoadReviewCard,
	summarizeInspectorChanges,
} from "./inspector-changes-presentation";

describe("inspector changes presentation", () => {
	it("summarizes the visible scope without leaking negative counters", () => {
		expect(
			summarizeInspectorChanges([
				{ insertions: 4, deletions: 2 },
				{ insertions: 7, deletions: 0 },
				{ insertions: -1, deletions: 3 },
			]),
		).toEqual({ fileCount: 3, insertions: 11, deletions: 5 });
	});

	it("opens the working tree when it has changes", () => {
		expect(defaultInspectorChangesScope(2, 8)).toBe("working");
	});

	it("opens the branch when the working tree is clean", () => {
		expect(defaultInspectorChangesScope(0, 8)).toBe("branch");
	});

	it("keeps an empty repository on the actionable working-tree scope", () => {
		expect(defaultInspectorChangesScope(0, 0)).toBe("working");
	});

	it("keeps file previews aligned with the selected scope", () => {
		expect(changeGroupBelongsToScope("staged", "working")).toBe(true);
		expect(changeGroupBelongsToScope("unstaged", "working")).toBe(true);
		expect(changeGroupBelongsToScope("committed", "working")).toBe(false);
		expect(changeGroupBelongsToScope("committed", "branch")).toBe(true);
		expect(changeGroupBelongsToScope("unstaged", "branch")).toBe(false);
	});

	it("sizes review cards for a continuous feed without letting one file dominate", () => {
		expect(reviewCardDiffHeight(1, 1)).toBe(226);
		expect(reviewCardDiffHeight(-10, 0)).toBe(190);
		expect(reviewCardDiffHeight(500, 300)).toBe(520);
	});

	it("eagerly loads only the first visible review cards", () => {
		expect(shouldEagerLoadReviewCard(0)).toBe(true);
		expect(shouldEagerLoadReviewCard(1)).toBe(true);
		expect(shouldEagerLoadReviewCard(2)).toBe(false);
		expect(shouldEagerLoadReviewCard(-1)).toBe(false);
	});
});
