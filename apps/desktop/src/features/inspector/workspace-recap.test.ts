import { describe, expect, it } from "vitest";
import { buildWorkspaceRecap, type WorkspaceRecapInput } from "./workspace-recap";

function input(overrides: Partial<WorkspaceRecapInput> = {}): WorkspaceRecapInput {
	return {
		commitMode: "create-pr",
		sessionActive: false,
		changedFilesCount: 0,
		additions: 0,
		deletions: 0,
		aheadOfRemoteCount: 0,
		conflictCount: 0,
		committedVsBaseCount: 0,
		prNumber: null,
		prState: null,
		requestLabel: "PR",
		pendingReviewFindingsCount: 0,
		...overrides,
	};
}

describe("buildWorkspaceRecap", () => {
	it("reports clean when there is nothing to act on", () => {
		const recap = buildWorkspaceRecap(input());
		expect(recap.messageKey).toBe("clean");
		expect(recap.action).toBeNull();
	});

	it("puts conflicts above everything else", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "resolve-conflicts",
				conflictCount: 2,
				sessionActive: true,
				pendingReviewFindingsCount: 4,
				changedFilesCount: 3,
			}),
		);
		expect(recap.messageKey).toBe("conflicts");
		expect(recap.tone).toBe("attention");
		expect(recap.action).toEqual({
			kind: "git",
			labelKey: "commit.modes.resolve-conflicts.idle",
		});
	});

	it("treats raw conflict count as conflicts even without the commit mode", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "commit-and-push", conflictCount: 1, changedFilesCount: 5 }),
		);
		expect(recap.messageKey).toBe("conflicts");
		expect(recap.params.count).toBe(1);
	});

	it("offers continue after a merge", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "merged", prNumber: 42, sessionActive: true }),
		);
		expect(recap.messageKey).toBe("merged");
		expect(recap.params.pr).toBe("PR #42");
		expect(recap.tone).toBe("done");
		expect(recap.action?.kind).toBe("continue");
	});

	it("reports a closed PR without inventing a next git step", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "open-pr", prState: "closed", prNumber: 7 }),
		);
		expect(recap.messageKey).toBe("closed");
		expect(recap.action).toEqual({ kind: "git", labelKey: "commit.modes.open-pr.idle" });
	});

	it("surfaces the streaming agent above findings and uncommitted changes", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				sessionActive: true,
				changedFilesCount: 3,
				additions: 153,
				deletions: 88,
				pendingReviewFindingsCount: 2,
			}),
		);
		expect(recap.messageKey).toBe("working");
		expect(recap.params).toEqual({ count: 3, additions: 153, deletions: 88 });
		expect(recap.tone).toBe("working");
		expect(recap.action?.kind).toBe("activity");
	});

	it("uses the no-changes variant while the agent has not edited anything", () => {
		const recap = buildWorkspaceRecap(input({ sessionActive: true }));
		expect(recap.messageKey).toBe("workingClean");
	});

	it("puts fresh review findings ahead of committing", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				changedFilesCount: 3,
				pendingReviewFindingsCount: 4,
			}),
		);
		expect(recap.messageKey).toBe("findings");
		expect(recap.params.count).toBe(4);
		expect(recap.action?.kind).toBe("review");
	});

	it("maps commit-and-push to the changes recap", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				changedFilesCount: 2,
				additions: 10,
				deletions: 4,
			}),
		);
		expect(recap.messageKey).toBe("changes");
		expect(recap.params).toEqual({ count: 2, additions: 10, deletions: 4 });
		expect(recap.action).toEqual({
			kind: "git",
			labelKey: "commit.modes.commit-and-push.idle",
		});
	});

	it("maps push to the ahead recap", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "push", aheadOfRemoteCount: 3 }),
		);
		expect(recap.messageKey).toBe("ahead");
		expect(recap.params.count).toBe(3);
	});

	it("maps merge to a ready-to-merge recap with the PR reference", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "merge", prNumber: 12, prState: "open" }),
		);
		expect(recap.messageKey).toBe("mergeReady");
		expect(recap.params.pr).toBe("PR #12");
		expect(recap.tone).toBe("ready");
		expect(recap.action).toEqual({ kind: "git", labelKey: "commit.modes.merge.idle" });
	});

	it("maps fix to failing checks with attention tone", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "fix", prNumber: 9, prState: "open", requestLabel: "MR" }),
		);
		expect(recap.messageKey).toBe("checksFailing");
		expect(recap.params.pr).toBe("MR #9");
		expect(recap.tone).toBe("attention");
	});

	it("falls back to the request label when the PR number is unknown", () => {
		const recap = buildWorkspaceRecap(input({ commitMode: "fix" }));
		expect(recap.params.pr).toBe("PR");
	});

	it("suggests creating a PR once commits exist against the base", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "create-pr", committedVsBaseCount: 5 }),
		);
		expect(recap.messageKey).toBe("readyForPr");
		expect(recap.params.count).toBe(5);
		expect(recap.action).toEqual({
			kind: "git",
			labelKey: "commit.modes.create-pr.idle",
		});
	});

	it("ignores review findings when the recap already ended in clean", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "create-pr", pendingReviewFindingsCount: 0 }),
		);
		expect(recap.messageKey).toBe("clean");
	});
});
