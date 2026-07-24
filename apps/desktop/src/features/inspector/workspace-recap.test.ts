import { describe, expect, it } from "vitest";
import {
	buildWorkspaceRecap,
	workspaceRecapActionForMode,
	type WorkspaceRecapAction,
	type WorkspaceRecapInput,
} from "./workspace-recap";

function input(overrides: Partial<WorkspaceRecapInput> = {}): WorkspaceRecapInput {
	return {
		commitMode: "create-pr",
		turnRunning: false,
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
		pendingDelegationResultsCount: 0,
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
				turnRunning: true,
				pendingReviewFindingsCount: 4,
				changedFilesCount: 3,
			}),
		);
		expect(recap.messageKey).toBe("conflicts");
		expect(recap.state).toBe("blocked");
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
			input({ commitMode: "merged", prNumber: 42, turnRunning: true }),
		);
		expect(recap.messageKey).toBe("merged");
		expect(recap.state).toBe("delivered");
		expect(recap.params.pr).toBe("PR #42");
		expect(recap.tone).toBe("done");
		expect(recap.action?.kind).toBe("continue");
	});

	it("reports a closed PR without inventing a next git step", () => {
		const recap = buildWorkspaceRecap(
			input({ commitMode: "open-pr", prState: "closed", prNumber: 7 }),
		);
		expect(recap.messageKey).toBe("closed");
		expect(recap.state).toBe("needs_attention");
		expect(recap.action).toEqual({ kind: "git", labelKey: "commit.modes.open-pr.idle" });
	});

	it("surfaces the streaming agent above findings and uncommitted changes", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				turnRunning: true,
				changedFilesCount: 3,
				additions: 153,
				deletions: 88,
				pendingReviewFindingsCount: 2,
			}),
		);
		expect(recap.messageKey).toBe("working");
		expect(recap.state).toBe("in_development");
		expect(recap.params).toEqual({ count: 3, additions: 153, deletions: 88 });
		expect(recap.tone).toBe("working");
		expect(recap.action?.kind).toBe("activity");
	});

	it("uses the no-changes variant while the agent has not edited anything", () => {
		const recap = buildWorkspaceRecap(input({ turnRunning: true }));
		expect(recap.messageKey).toBe("workingClean");
	});

	it("returns to the git action after the turn finishes", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				turnRunning: false,
				changedFilesCount: 2,
			}),
		);
		expect(recap.messageKey).toBe("changes");
		expect(recap.action?.kind).toBe("git");
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

	it("surfaces completed delegation results before committing", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "commit-and-push",
				changedFilesCount: 3,
				pendingDelegationResultsCount: 2,
				pendingReviewFindingsCount: 4,
			}),
		);
		expect(recap.messageKey).toBe("delegationResults");
		expect(recap.params.count).toBe(2);
		expect(recap.tone).toBe("ready");
		expect(recap.action?.kind).toBe("activity");
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
		expect(recap.state).toBe("ready_to_deliver");
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

	it("marks an open change request as awaiting review", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "open-pr",
				prNumber: 31,
				prState: "open",
			}),
		);

		expect(recap.messageKey).toBe("prOpen");
		expect(recap.state).toBe("awaiting_review");
	});

	it("surfaces a captured delivery failure without hiding an active agent turn", () => {
		const failure = {
			operation: "push" as const,
			classification: "authentication",
		};
		const failed = buildWorkspaceRecap(
			input({ commitMode: "merge", prNumber: 9, deliveryFailure: failure }),
		);
		const recovering = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 9,
				deliveryFailure: failure,
				turnRunning: true,
			}),
		);

		expect(failed.state).toBe("needs_attention");
		expect(failed.messageKey).toBe("deliveryFailure.push");
		expect(failed.action?.kind).toBe("delivery");
		expect(recovering.state).toBe("in_development");
		expect(recovering.messageKey).toBe("workingClean");
	});

	it("puts review conflicts and failed pipelines ahead of merge readiness", () => {
		const reviewBlocked = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 12,
				reviewState: {
					reviewState: "pending",
					approvalsAvailable: true,
					approvalsLeft: 1,
					hasConflicts: true,
					behindBy: 0,
					discussionsResolved: true,
					draft: false,
				},
				pipeline: { status: "failed", failedJobs: 2 },
			}),
		);
		const pipelineBlocked = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 12,
				pipeline: { status: "failed", failedJobs: 2 },
			}),
		);

		expect(reviewBlocked.messageKey).toBe("reviewConflicts");
		expect(reviewBlocked.state).toBe("blocked");
		expect(pipelineBlocked.messageKey).toBe("pipelineFailed");
		expect(pipelineBlocked.state).toBe("blocked");
		expect(pipelineBlocked.action?.kind).toBe("pipeline");
	});

	it("never treats unavailable approval or pipeline information as approval", () => {
		const approvalsUnavailable = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 18,
				reviewState: {
					reviewState: "unknown",
					approvalsAvailable: false,
					approvalsLeft: null,
					hasConflicts: false,
					behindBy: 0,
					discussionsResolved: true,
					draft: false,
				},
			}),
		);
		const pipelineUnavailable = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 18,
				pipelineStatus: "error",
			}),
		);

		expect(approvalsUnavailable.state).toBe("awaiting_review");
		expect(approvalsUnavailable.messageKey).toBe("approvalsUnavailable");
		expect(pipelineUnavailable.state).toBe("needs_attention");
		expect(pipelineUnavailable.messageKey).toBe("pipelineUnavailable");
	});

	it("keeps ready only after the available review and pipeline signals pass", () => {
		const recap = buildWorkspaceRecap(
			input({
				commitMode: "merge",
				prNumber: 24,
				reviewStatus: "available",
				reviewState: {
					reviewState: "approved",
					approvalsAvailable: true,
					approvalsLeft: 0,
					hasConflicts: false,
					behindBy: 0,
					discussionsResolved: true,
					draft: false,
				},
				pipelineStatus: "available",
				pipeline: { status: "success", failedJobs: 0 },
			}),
		);

		expect(recap.state).toBe("ready_to_deliver");
		expect(recap.messageKey).toBe("mergeReady");
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
		expect(recap.state).toBe("in_development");
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

describe("workspaceRecapActionForMode", () => {
	const gitAction: WorkspaceRecapAction = {
		kind: "git",
		labelKey: "commit.modes.create-pr.idle",
	};

	it("hides a duplicate Git CTA while the Changes surface is visible", () => {
		expect(workspaceRecapActionForMode(gitAction, "git")).toBeNull();
	});

	it("turns a Git CTA into navigation while the Files surface is visible", () => {
		expect(workspaceRecapActionForMode(gitAction, "code")).toEqual({
			kind: "git",
			labelKey: "inspector.recap.actions.changes",
		});
	});

	it("leaves the post-merge Continue CTA only in the visible Git header", () => {
		const continueAction: WorkspaceRecapAction = {
			kind: "continue",
			labelKey: "inspector.recap.actions.continue",
		};
		expect(workspaceRecapActionForMode(continueAction, "git")).toBeNull();
		expect(workspaceRecapActionForMode(continueAction, "code")).toBe(continueAction);
	});

	it("preserves recap actions that are not duplicated by the Git header", () => {
		const reviewAction: WorkspaceRecapAction = {
			kind: "review",
			labelKey: "inspector.recap.actions.review",
		};
		expect(workspaceRecapActionForMode(reviewAction, "git")).toBe(reviewAction);
	});
});
