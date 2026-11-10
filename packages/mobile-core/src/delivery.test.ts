import { describe, expect, it } from "vitest";
import {
	changeTotals,
	deliveryActionLabel,
	deliveryActions,
	pipelineText,
	type DeliverySummary,
} from "./delivery.ts";

describe("delivery", () => {
	const summary = (extra: Partial<DeliverySummary> = {}): DeliverySummary => ({
		workspaceId: "w",
		workspaceState: "ready",
		branch: "dcc/x",
		baseBranch: "main",
		changes: [{ path: "a.ts", status: "M", insertions: 3, deletions: 1, staged: false }],
		stagedCount: 0,
		branchChanges: [],
		aheadOfRemoteCount: 0,
		behindOfRemoteCount: 0,
		conflictCount: 0,
		mergeInProgress: false,
		changesToken: "t",
		commitMessage: "chore: update a",
		beforePush: [],
		pushFailure: null,
		...extra,
	});

	it("keeps the phone to commit, push and PR, and says when the next step is the desktop's", () => {
		const local = deliveryActions(summary(), null, false);
		expect(local.mode).toBe("commit-and-push");
		expect(local.actions.map((a) => a.id)).toEqual(["commit-and-push", "commit", "push", "sync-base", "create-pr", "create-draft-pr"]);
		expect(local.actions.find((a) => a.id === "push")?.disabled).toBe(true);
		const pushed = deliveryActions(summary({ changes: [], branchChanges: [{ path: "a.ts", status: "M", insertions: 1, deletions: 0, staged: false }] }), null, false);
		expect(pushed.mode).toBe("create-pr");
		expect(pushed.actions[0]?.id).toBe("create-pr");
		expect(local.primaryId).toBe("commit-and-push");
		const conflicts = deliveryActions(summary({ conflictCount: 2 }), null, false);
		expect(conflicts.desktopOnly).toMatch(/conflitos/);
		expect(conflicts.primaryId).toBeNull();
		// "fix" from the branch name alone (the desktop's heuristic): no PR, no warning, no highlight.
		const named = deliveryActions(summary({ branch: "dcc/fixture", changes: [], branchChanges: [{ path: "a.ts", status: "A", insertions: 1, deletions: 0, staged: false }] }), null, false);
		expect(named.mode).toBe("fix");
		expect(named.primaryId).toBeNull();
		expect(named.desktopOnly).toBeNull();
		const open = deliveryActions(summary({ changes: [] }), { pullRequest: { state: "OPEN", headBranch: "dcc/x", mergeStateStatus: "CLEAN" }, pipeline: null, requestTitle: null }, false);
		expect(open.mode).toBe("merge");
		expect(open.actions.map((a) => a.id)).toContain("open-pr");
		// Merging is the phone's next step too, and nothing sends it to the desktop.
		expect(open.primaryId).toBe("merge");
		expect(open.desktopOnly).toBeNull();
		expect(deliveryActionLabel(open.actions[0]!)).toBe("Mesclar");
		// Failing checks keep the merge off; open conflicts keep the base sync off.
		const failing = deliveryActions(summary({ changes: [] }), { pullRequest: { state: "OPEN", headBranch: "dcc/x", mergeStateStatus: "UNSTABLE" }, pipeline: null, requestTitle: null }, false);
		expect(failing.actions.find((a) => a.id === "merge")?.disabled).toBe(true);
		expect(local.actions.find((a) => a.id === "sync-base")?.disabled).toBe(false);
		expect(conflicts.actions.find((a) => a.id === "sync-base")?.disabled).toBe(true);
	});

	it("sums the diff and words the CI", () => {
		expect(changeTotals(summary().changes)).toEqual({ files: 1, additions: 3, deletions: 1 });
		expect(pipelineText({ status: "failed", webUrl: null, failedJobs: ["lint", "test"] })).toBe("CI falhou: lint, test");
		expect(pipelineText({ status: "success", webUrl: null, failedJobs: [] })).toBe("CI passou");
	});
});
