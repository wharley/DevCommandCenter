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
