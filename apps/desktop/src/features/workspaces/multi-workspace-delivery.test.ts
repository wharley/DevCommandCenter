import { describe, expect, it, vi } from "vitest";
import type {
	MultiWorkspaceDeliveryDependencies,
	MultiWorkspaceDeliveryMember,
} from "./multi-workspace-delivery";
import {
	deliverMultiWorkspace,
	resolveMultiWorkspaceDeliveryState,
} from "./multi-workspace-delivery";

const member: MultiWorkspaceDeliveryMember = {
	workspaceId: "workspace-api",
	name: "service-api",
	workspaceRoot: "/worktrees/service-api",
};

function change(path: string) {
	return {
		path,
		name: path.split("/").at(-1) ?? path,
		absolutePath: `/worktrees/service-api/${path}`,
		status: "M",
		insertions: 1,
		deletions: 0,
	};
}

function status(overrides: Record<string, unknown> = {}) {
	return {
		staged: [],
		unstaged: [],
		currentBranch: "dcc/task-api",
		aheadOfRemoteCount: 0,
		behindOfRemoteCount: 0,
		conflictCount: 0,
		mergeInProgress: false,
		...overrides,
	};
}

function dependencies(
	overrides: Partial<MultiWorkspaceDeliveryDependencies> = {},
): MultiWorkspaceDeliveryDependencies {
	return {
		gitStatus: vi.fn().mockResolvedValue(status()),
		branchDiff: vi.fn().mockResolvedValue({ changes: [], baseBranch: "main" }),
		projectAutomation: vi.fn().mockResolvedValue({
			setupCommand: null,
			tasks: [],
			beforeMerge: [],
			beforePush: [],
			sourcePath: ".dcc/project.json",
			configHash: null,
			trackedInGit: false,
		}),
		runProjectTasks: vi.fn().mockResolvedValue({
			report: { status: "passed", sourcePath: null, steps: [] },
			changedFiles: false,
		}),
		stageAll: vi.fn().mockResolvedValue(undefined),
		commitPush: vi.fn().mockResolvedValue(undefined),
		push: vi.fn().mockResolvedValue(undefined),
		requestStatus: vi.fn().mockResolvedValue({ state: null, url: null }),
		createRequest: vi.fn().mockResolvedValue(undefined),
		...overrides,
	};
}

describe("deliverMultiWorkspace", () => {
	it("does not offer delivery again for a clean branch with an open PR", () => {
		expect(
			resolveMultiWorkspaceDeliveryState({
				gitStatus: status(),
				branchDiff: { changes: [change("route.ts")], baseBranch: "main" },
				requestState: "open",
			}),
		).toEqual({ hasChanges: true, needsDelivery: false });
	});

	it("offers delivery again when an open PR has new local work", () => {
		expect(
			resolveMultiWorkspaceDeliveryState({
				gitStatus: status({ unstaged: [change("route.ts")] }),
				branchDiff: { changes: [change("route.ts")], baseBranch: "main" },
				requestState: "open",
			}),
		).toEqual({ hasChanges: true, needsDelivery: true });
	});

	it("offers delivery for an unpublished branch diff", () => {
		expect(
			resolveMultiWorkspaceDeliveryState({
				gitStatus: status(),
				branchDiff: { changes: [change("route.ts")], baseBranch: "main" },
				requestState: null,
			}),
		).toEqual({ hasChanges: true, needsDelivery: true });
	});

	it("skips a project with no changes without publishing anything", async () => {
		const deps = dependencies();
		const [result] = await deliverMultiWorkspace([member], deps);

		expect(result.status).toBe("skipped");
		expect(result.action).toBe("no-changes");
		expect(deps.commitPush).not.toHaveBeenCalled();
		expect(deps.createRequest).not.toHaveBeenCalled();
	});

	it("stages all local changes, publishes them and creates a PR", async () => {
		const deps = dependencies({
			gitStatus: vi
				.fn()
				.mockResolvedValueOnce(status({ unstaged: [change("route.ts")] }))
				.mockResolvedValueOnce(status({ unstaged: [change("route.ts")] }))
				.mockResolvedValue(status()),
			branchDiff: vi
				.fn()
				.mockResolvedValueOnce({ changes: [], baseBranch: "main" })
				.mockResolvedValue({ changes: [change("route.ts")], baseBranch: "main" }),
		});

		const [result] = await deliverMultiWorkspace([member], deps);

		expect(deps.stageAll).toHaveBeenCalledWith(member.workspaceRoot);
		expect(deps.commitPush).toHaveBeenCalledWith(
			member.workspaceRoot,
			"chore: checkpoint for service-api",
		);
		expect(deps.createRequest).toHaveBeenCalledWith(member.workspaceRoot);
		expect(result.status).toBe("delivered");
		expect(result.action).toBe("created-request");
	});

	it("updates an existing open PR instead of creating a duplicate", async () => {
		const deps = dependencies({
			gitStatus: vi
				.fn()
				.mockResolvedValueOnce(status({ aheadOfRemoteCount: 1 }))
				.mockResolvedValueOnce(status({ aheadOfRemoteCount: 1 }))
				.mockResolvedValue(status()),
			branchDiff: vi.fn().mockResolvedValue({
				changes: [change("page.tsx")],
				baseBranch: "main",
			}),
			requestStatus: vi.fn().mockResolvedValue({
				state: "open",
				url: "https://example.test/pr/42",
			}),
		});

		const [result] = await deliverMultiWorkspace([member], deps);

		expect(deps.push).toHaveBeenCalledWith(member.workspaceRoot);
		expect(deps.createRequest).not.toHaveBeenCalled();
		expect(result.action).toBe("updated-request");
		expect(result.requestUrl).toBe("https://example.test/pr/42");
	});

	it("keeps processing other projects after a beforePush failure", async () => {
		const first = dependencies({
			gitStatus: vi.fn().mockResolvedValue(status({ unstaged: [change("bad.ts")] })),
			projectAutomation: vi.fn().mockResolvedValue({
				setupCommand: null,
				tasks: [],
				beforeMerge: [],
				beforePush: ["test"],
				sourcePath: ".dcc/project.json",
				configHash: "hash",
				trackedInGit: true,
			}),
			runProjectTasks: vi.fn().mockResolvedValue({
				report: { status: "failed", sourcePath: null, steps: [] },
				changedFiles: false,
			}),
		});
		const second = dependencies();
		const combined: MultiWorkspaceDeliveryDependencies = {
			...first,
			gitStatus: async (root) =>
				root.includes("service-api")
					? first.gitStatus(root)
					: second.gitStatus(root),
			branchDiff: async (root) =>
				root.includes("service-api")
					? first.branchDiff(root)
					: second.branchDiff(root),
		};
		const cleanMember = {
			workspaceId: "workspace-app",
			name: "zedy-app",
			workspaceRoot: "/worktrees/zedy-app",
		};

		const results = await deliverMultiWorkspace([member, cleanMember], combined);

		expect(results.map((result) => result.status)).toEqual(["failed", "skipped"]);
		expect(second.gitStatus).toHaveBeenCalled();
	});

	it("blocks conflicted projects before any publication", async () => {
		const deps = dependencies({
			gitStatus: vi.fn().mockResolvedValue(status({ conflictCount: 1 })),
		});

		const [result] = await deliverMultiWorkspace([member], deps);

		expect(result.status).toBe("failed");
		expect(result.message).toContain("conflitos");
		expect(deps.stageAll).not.toHaveBeenCalled();
	});
});
