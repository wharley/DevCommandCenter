import type { WorkspaceCleanupEntry, WorkspaceCleanupSafety } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import {
	completedCleanupRows,
	isPreselectedForCleanup,
	selectedCleanupBytes,
} from "./completed-cleanup";
import type { WorkspaceSummary } from "./types";

function workspace(id: string, memberWorkspaceIds?: string[]): WorkspaceSummary {
	return { id, name: id, branch: id, status: "completed", memberWorkspaceIds };
}

function entry(
	workspaceId: string,
	bytes: number,
	safety: WorkspaceCleanupSafety,
	extra: Partial<WorkspaceCleanupEntry> = {},
): WorkspaceCleanupEntry {
	return { workspaceId, bytes, safety, changedFiles: 0, unpushedCommits: 0, ...extra };
}

describe("completedCleanupRows", () => {
	it("orders tasks by size, largest first", () => {
		const rows = completedCleanupRows(
			[workspace("small"), workspace("large")],
			[entry("small", 10, "safe"), entry("large", 500, "safe")],
		);
		expect(rows.map((row) => row.workspace.id)).toEqual(["large", "small"]);
	});

	it("folds bundle members into the riskiest verdict and the summed size", () => {
		const [row] = completedCleanupRows(
			[workspace("bundle", ["a", "b"])],
			[
				entry("a", 100, "safe"),
				entry("b", 50, "uncommitted", { changedFiles: 3 }),
			],
		);
		expect(row).toMatchObject({ bytes: 150, safety: "uncommitted", changedFiles: 3 });
	});

	it("a member without a worktree does not hide a safe one", () => {
		const [row] = completedCleanupRows(
			[workspace("bundle", ["a", "b"])],
			[entry("a", 0, "noWorktree"), entry("b", 80, "safe")],
		);
		expect(row.safety).toBe("safe");
	});

	it("a task the scan did not return is unknown", () => {
		const [row] = completedCleanupRows([workspace("gone")], []);
		expect(row.safety).toBe("unknown");
	});
});

describe("cleanup selection", () => {
	it("preselects only safe tasks that free space", () => {
		const rows = completedCleanupRows(
			[workspace("safe"), workspace("empty"), workspace("dirty"), workspace("ahead")],
			[
				entry("safe", 100, "safe"),
				entry("empty", 0, "safe"),
				entry("dirty", 100, "uncommitted"),
				entry("ahead", 100, "unpushed"),
			],
		);
		expect(rows.filter(isPreselectedForCleanup).map((row) => row.workspace.id)).toEqual([
			"safe",
		]);
	});

	it("sums the size of the selected tasks", () => {
		const rows = completedCleanupRows(
			[workspace("a"), workspace("b"), workspace("c")],
			[entry("a", 100, "safe"), entry("b", 40, "safe"), entry("c", 7, "safe")],
		);
		expect(selectedCleanupBytes(rows, new Set(["a", "c"]))).toBe(107);
	});
});
