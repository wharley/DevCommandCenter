import { describe, expect, it } from "vitest";
import {
	resolveExecutionDockStatus,
	type ExecutionDockChangeSummary,
} from "./ExecutionDock.logic";

const cleanSummary: ExecutionDockChangeSummary = {
	files: 0,
	additions: 0,
	deletions: 0,
	branchFiles: 0,
	branchAdditions: 0,
	branchDeletions: 0,
	aheadOfRemoteCount: 0,
};

describe("resolveExecutionDockStatus", () => {
	it("keeps a new clean task as no changes", () => {
		expect(resolveExecutionDockStatus(cleanSummary, "ready")).toEqual({ kind: "none" });
	});

	it("shows the committed branch diff instead of no changes", () => {
		expect(
			resolveExecutionDockStatus(
				{ ...cleanSummary, branchFiles: 4, branchAdditions: 20, branchDeletions: 3 },
				"ready",
			),
		).toEqual({ kind: "branch", files: 4, additions: 20, deletions: 3 });
	});

	it("keeps local changes visible when both sources exist", () => {
		expect(
			resolveExecutionDockStatus(
				{ ...cleanSummary, files: 2, additions: 8, branchFiles: 5 },
				"ready",
			),
		).toEqual({ kind: "local-and-branch", localFiles: 2, branchFiles: 5 });
	});

	it("reports a merged pull request when the worktree is clean", () => {
		expect(
			resolveExecutionDockStatus(
				{ ...cleanSummary, pullRequestState: "MERGED", pullRequestNumber: 42 },
				"ready",
			),
		).toEqual({ kind: "merged", pullRequestNumber: 42 });
	});

	it("does not claim no changes while Git data is loading", () => {
		expect(resolveExecutionDockStatus(null, "loading")).toEqual({ kind: "loading" });
	});
});
