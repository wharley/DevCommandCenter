import { describe, expect, it } from "vitest";
import {
	isInlineGitDiffReview,
	shouldReturnToGitFiles,
} from "./git-diff-review-navigation";

describe("git diff review navigation", () => {
	it("uses the focused Inspector before expansion", () => {
		expect(isInlineGitDiffReview("git-diff", false)).toBe(true);
		expect(isInlineGitDiffReview("git-diff", true)).toBe(false);
		expect(isInlineGitDiffReview("turn-review", false)).toBe(false);
	});

	it("returns an expanded diff to the changed-files list", () => {
		expect(shouldReturnToGitFiles("git-diff", true)).toBe(true);
		expect(shouldReturnToGitFiles("git-diff", false)).toBe(false);
		expect(shouldReturnToGitFiles("turn-review", true)).toBe(false);
	});
});
