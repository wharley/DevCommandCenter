import { describe, expect, it } from "vitest";
import { resolveCommitMode } from "./WorkspaceCommitButton.logic";

describe("resolveCommitMode", () => {
	it("defaults to create-pr when there is no stronger signal", () => {
		expect(resolveCommitMode("feature/statuslane")).toBe("create-pr");
	});

	it("keeps special branch-based modes", () => {
		expect(resolveCommitMode("fix/statuslane")).toBe("fix");
		expect(resolveCommitMode("release/push")).toBe("push");
		expect(resolveCommitMode("hotfix/merge")).toBe("merge");
		expect(resolveCommitMode("pr/statuslane")).toBe("open-pr");
	});

	it("prefers real PR state over branch-name heuristics", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "MERGEABLE", mergeStateStatus: "CLEAN" },
				gitStatus: { staged: [], unstaged: [], aheadOfRemoteCount: 0, behindOfRemoteCount: 0, conflictCount: 0 },
			}),
		).toBe("merge");
	});

	it("shows resolve-conflicts when the PR is open and mergeable is conflicting", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "CONFLICTING", mergeStateStatus: "BLOCKED" },
				gitStatus: { staged: [], unstaged: [], aheadOfRemoteCount: 0, behindOfRemoteCount: 0, conflictCount: 0 },
			}),
		).toBe("resolve-conflicts");
	});

	it("allows a staged conflict resolution to be committed while the forge is still conflicting", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "CONFLICTING", mergeStateStatus: "DIRTY" },
				gitStatus: {
					staged: [{}],
					unstaged: [],
					aheadOfRemoteCount: 0,
					behindOfRemoteCount: 0,
					conflictCount: 0,
				},
			}),
		).toBe("commit-and-push");
	});

	it("returns to the resolver when a merge is still in progress after all files were staged", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "CONFLICTING" },
				gitStatus: {
					staged: [{}],
					unstaged: [],
					aheadOfRemoteCount: 0,
					conflictCount: 0,
					mergeInProgress: true,
				},
			}),
		).toBe("resolve-conflicts");
	});

	it("keeps unresolved local conflicts ahead of staged changes", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "CONFLICTING" },
				gitStatus: {
					staged: [{}],
					unstaged: [{}],
					aheadOfRemoteCount: 0,
					behindOfRemoteCount: 0,
					conflictCount: 1,
				},
			}),
		).toBe("resolve-conflicts");
	});

	it("allows a local resolution commit to be pushed while the forge is still conflicting", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "CONFLICTING", mergeStateStatus: "DIRTY" },
				gitStatus: {
					staged: [],
					unstaged: [],
					aheadOfRemoteCount: 1,
					behindOfRemoteCount: 0,
					conflictCount: 0,
				},
			}),
		).toBe("push");
	});

	it("shows commit-and-push when the PR is open and the worktree is dirty", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "MERGEABLE", mergeStateStatus: "CLEAN" },
				gitStatus: { staged: [{}], unstaged: [], aheadOfRemoteCount: 0, behindOfRemoteCount: 0, conflictCount: 0 },
			}),
		).toBe("commit-and-push");
	});

	it("shows push when the PR is open and local commits are ahead of remote", () => {
		expect(
			resolveCommitMode({
				branch: "feature/statuslane",
				prStatus: { state: "open", mergeable: "MERGEABLE", mergeStateStatus: "CLEAN" },
				gitStatus: { staged: [], unstaged: [], aheadOfRemoteCount: 2, behindOfRemoteCount: 0, conflictCount: 0 },
			}),
		).toBe("push");
	});

	it("shows merged and closed terminal states", () => {
		expect(
			resolveCommitMode({ branch: "feature/statuslane", prStatus: { state: "merged" } }),
		).toBe("merged");
		expect(
			resolveCommitMode({ branch: "feature/statuslane", prStatus: { state: "closed" } }),
		).toBe("open-pr");
	});
});
