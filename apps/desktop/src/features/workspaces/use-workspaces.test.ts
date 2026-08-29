import { describe, expect, it } from "vitest";
import {
	removeWorkspacesFromList,
	workspaceMutationIds,
	workspaceToSummary,
} from "./use-workspaces";

describe("workspaceToSummary", () => {
	it("treats a recommended setup report as ready instead of blocking the task", () => {
		const summary = workspaceToSummary({
			id: "recommended-setup",
			projectId: "widgets",
			name: "Ready task",
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: "/repo/.dcc-worktrees/recommended-setup",
			source: null,
			state: "setup_pending",
			setupReport: {
				status: "pending",
				steps: [
					{
						label: "Install dependencies",
						command: "yarn install",
						sourcePath: "/repo/widgets/package.json",
						status: "pending",
						detail: null,
					},
				],
				message: "Setup is recommended.",
			},
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.status).toBe("ready");
	});

	it("keeps a failed setup visible as a workspace problem", () => {
		const summary = workspaceToSummary({
			id: "failed-setup",
			projectId: "widgets",
			name: "Needs attention",
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: "/repo/.dcc-worktrees/failed-setup",
			source: null,
			state: "setup_pending",
			setupReport: {
				status: "failed",
				steps: [],
				message: "Setup failed.",
			},
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.status).toBe("setup_pending");
	});

	it("does not keep a local-direct task blocked by a legacy setup failure", () => {
		const summary = workspaceToSummary({
			id: "local-legacy-setup",
			projectId: "widgets",
			name: "Local task",
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: null,
			source: null,
			state: "setup_pending",
			setupReport: {
				status: "failed",
				steps: [],
				message: "workspace mutation is unavailable",
			},
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.status).toBe("ready");
	});

	it("keeps the base branch out of an untitled task name", () => {
		const summary = workspaceToSummary({
			id: "blank-1",
			projectId: "widgets",
			name: null,
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: "/repo/.dcc-worktrees/blank-1",
			source: null,
			state: "ready",
			setupReport: null,
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.name).toBe("Nova tarefa");
		expect(summary.isAutoNamed).toBe(true);
		expect(summary.branch).toBe("main");
	});

	it("keeps a persisted provisional name eligible for the first-prompt title", () => {
		const summary = workspaceToSummary({
			id: "placeholder-1",
			projectId: "widgets",
			name: "Nova tarefa",
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: "/repo/.dcc-worktrees/placeholder-1",
			source: null,
			state: "ready",
			setupReport: null,
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.name).toBe("Nova tarefa");
		expect(summary.isAutoNamed).toBe(true);
	});

	it("uses the imported branch as the active workspace branch", () => {
		const summary = workspaceToSummary({
			id: "review-42",
			projectId: "widgets",
			name: null,
			rootPath: "/repo/widgets",
			baseBranch: "main",
			worktreePath: "/repo/.dcc-worktrees/review-42",
			source: {
				kind: "pull_request",
				url: "https://github.com/acme/widgets/pull/42",
				provider: "github",
				remoteName: "origin",
				headBranch: "feature/review",
				headSha: "abc123",
				baseBranch: "main",
				changeRequestNumber: 42,
				title: "Improve review flow",
				author: "octocat",
			},
			state: "ready",
			setupReport: null,
			pinnedAt: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		});

		expect(summary.name).toBe("Improve review flow");
		expect(summary.isAutoNamed).toBe(false);
		expect(summary.branch).toBe("feature/review");
	});

	it("preserves completed workspaces for the cleanup section", () => {
		const summary = workspaceToSummary(
			{
				id: "completed-1",
				projectId: "widgets",
				name: "Finished task",
				rootPath: "/repo/widgets",
				baseBranch: "main",
				worktreePath: "/repo/.dcc-worktrees/completed-1",
				source: null,
				state: "completed",
				setupReport: null,
				pinnedAt: null,
				createdAt: "2026-01-01T00:00:00Z",
				updatedAt: "2026-01-01T00:05:00Z",
			},
			{ remote: "origin", branch: "feature/finished" },
		);

		expect(summary.status).toBe("completed");
		expect(summary.remoteDeletionTargets).toEqual([
			{ remote: "origin", branch: "feature/finished" },
		]);
	});
});

describe("removeWorkspacesFromList", () => {
	it("keeps the current selection when it is not removed", () => {
		const result = removeWorkspacesFromList(
			[
				{ id: "a", name: "Alpha", branch: "main", status: "ready" },
				{ id: "b", name: "Beta", branch: "feat/beta", status: "ready" },
			],
			["a"],
			"b",
		);

		expect(result.workspaceList.map((workspace) => workspace.id)).toEqual(["b"]);
		expect(result.selectedWorkspaceId).toBe("b");
	});

	it("falls back to the first remaining workspace when the selected one is removed", () => {
		const result = removeWorkspacesFromList(
			[
				{ id: "a", name: "Alpha", branch: "main", status: "ready" },
				{ id: "b", name: "Beta", branch: "feat/beta", status: "ready" },
				{ id: "c", name: "Gamma", branch: "feat/gamma", status: "archived" },
			],
			["b", "c"],
			"b",
		);

		expect(result.workspaceList.map((workspace) => workspace.id)).toEqual(["a"]);
		expect(result.selectedWorkspaceId).toBe("a");
	});

	it("clears the selection when no workspaces remain", () => {
		const result = removeWorkspacesFromList(
			[{ id: "a", name: "Alpha", branch: "main", status: "ready" }],
			["a"],
			"a",
		);

		expect(result.workspaceList).toEqual([]);
		expect(result.selectedWorkspaceId).toBeNull();
	});
});

describe("workspaceMutationIds", () => {
	it("returns every member when removing a bundle", () => {
		expect(
			workspaceMutationIds(
				{
					id: "primary",
					name: "Task",
					branch: "dcc/task",
					status: "ready",
					bundleId: "bundle-1",
					memberWorkspaceIds: ["primary", "api", "web"],
				},
				"primary",
			),
		).toEqual(["primary", "api", "web"]);
	});

	it("keeps single-workspace deletion unchanged", () => {
		expect(workspaceMutationIds(undefined, "single")).toEqual(["single"]);
	});
});
