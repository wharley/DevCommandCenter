import { describe, expect, it } from "vitest";
import {
	changeTotals,
	deliveryActions,
	pipelineText,
	resolveCommitMode,
	resolveExecutionDockActions,
	type CommitModeContext,
	type DeliverySummary,
	type ExecutionDockActionContext,
} from "./delivery.ts";

const desktopRoot = new URL("../../../apps/desktop/src/", import.meta.url);

const pr = (state: string, extra: Record<string, unknown> = {}) => ({ state, headBranch: "dcc/x", ...extra });
const git = (staged: number, unstaged: number, extra: Record<string, unknown> = {}) => ({
	staged: Array(staged).fill({}),
	unstaged: Array(unstaged).fill({}),
	...extra,
});

const modeCases: CommitModeContext[] = [
	{ branch: "dcc/x" },
	{ branch: "dcc/x", gitStatus: git(1, 0) },
	{ branch: "dcc/x", gitStatus: git(0, 0, { aheadOfRemoteCount: 2 }) },
	{ branch: "dcc/x", gitStatus: git(0, 0, { mergeInProgress: true, conflictCount: 2 }) },
	{ branch: "dcc/x", gitStatus: git(0, 0, { mergeInProgress: true, conflictCount: 0 }) },
	{ branch: "dcc/x", gitStatus: git(0, 1, { conflictCount: 1 }) },
	{ branch: "dcc/x", prStatus: pr("OPEN"), gitStatus: git(0, 2) },
	{ branch: "dcc/x", prStatus: pr("OPEN"), gitStatus: git(0, 0, { aheadOfRemoteCount: 1 }) },
	{ branch: "dcc/x", prStatus: pr("OPEN", { mergeable: "CONFLICTING" }), gitStatus: git(0, 0) },
	{ branch: "dcc/x", prStatus: pr("OPEN", { mergeStateStatus: "unstable" }), gitStatus: git(0, 0) },
	{ branch: "dcc/x", prStatus: pr("OPEN", { mergeStateStatus: "CLEAN" }), gitStatus: git(0, 0) },
	{ branch: "dcc/x", prStatus: pr("MERGED"), gitStatus: git(0, 0) },
	{ branch: "dcc/x", prStatus: pr("CLOSED"), gitStatus: git(0, 0) },
	{ branch: "dcc/x", prStatus: pr("OPEN", { headBranch: "other" }), gitStatus: git(0, 0) },
	{ branch: "fix/login-merged" },
	{ branch: "pr/42" },
];

const actionCases: ExecutionDockActionContext[] = [];
for (const mode of ["create-pr", "commit-and-push", "push", "merge", "fix", "open-pr", null] as const) {
	for (const flags of [
		[false, false, false, false, false],
		[true, true, false, false, false],
		[false, true, true, true, true],
		[true, false, true, true, false],
	]) {
		const [hasLocalChanges, hasBranchChanges, hasAheadCommits, hasChangeRequest, hasOpenRequest] = flags as boolean[];
		actionCases.push({ mode, loading: false, multiProject: false, hasLocalChanges, hasBranchChanges, hasAheadCommits, hasChangeRequest, hasOpenRequest } as ExecutionDockActionContext);
	}
}
actionCases.push({ ...actionCases[0]!, loading: true }, { ...actionCases[0]!, multiProject: true });

describe("delivery", () => {
	it("picks the next step exactly like the desktop", async () => {
		const desktop = (await import(/* @vite-ignore */ new URL("features/commit/WorkspaceCommitButton.logic.ts", desktopRoot).href)) as {
			resolveCommitMode(input: unknown): string;
		};
		for (const context of modeCases) expect(resolveCommitMode(context), JSON.stringify(context)).toBe(desktop.resolveCommitMode(context));
	});

	it("offers the Dock's actions exactly like the desktop", async () => {
		const desktop = (await import(/* @vite-ignore */ new URL("features/composer/ExecutionDock.actions.ts", desktopRoot).href)) as {
			resolveExecutionDockActions(input: unknown): unknown;
		};
		for (const context of actionCases) {
			expect(resolveExecutionDockActions(context), JSON.stringify(context)).toEqual(desktop.resolveExecutionDockActions(context));
		}
	});

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
		expect(local.actions.map((a) => a.id)).toEqual(["commit-and-push", "commit", "push", "create-pr", "create-draft-pr"]);
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
	});

	it("sums the diff and words the CI", () => {
		expect(changeTotals(summary().changes)).toEqual({ files: 1, additions: 3, deletions: 1 });
		expect(pipelineText({ status: "failed", webUrl: null, failedJobs: ["lint", "test"] })).toBe("CI falhou: lint, test");
		expect(pipelineText({ status: "success", webUrl: null, failedJobs: [] })).toBe("CI passou");
	});
});
