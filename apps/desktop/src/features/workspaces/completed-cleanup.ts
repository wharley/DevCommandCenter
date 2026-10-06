import type { WorkspaceCleanupEntry, WorkspaceCleanupSafety } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import { workspaceDiskUsageIds } from "./workspace-disk-usage";

export type CompletedCleanupRow = {
	workspace: WorkspaceSummary;
	bytes: number;
	safety: WorkspaceCleanupSafety;
	changedFiles: number;
	unpushedCommits: number;
};

/** A multi-repo task is as risky as its riskiest member. */
const SAFETY_RANK: Record<WorkspaceCleanupSafety, number> = {
	noWorktree: 0,
	safe: 1,
	unpushed: 2,
	unknown: 3,
	uncommitted: 4,
};

/** One row per completed task, members of a bundle folded in, largest first. */
export function completedCleanupRows(
	workspaces: readonly WorkspaceSummary[],
	entries: readonly WorkspaceCleanupEntry[],
): CompletedCleanupRow[] {
	const byId = new Map(entries.map((entry) => [entry.workspaceId, entry]));
	return workspaces
		.map((workspace) => {
			const members = workspaceDiskUsageIds([workspace]).map((id) => byId.get(id));
			const row: CompletedCleanupRow = {
				workspace,
				bytes: 0,
				safety: "noWorktree",
				changedFiles: 0,
				unpushedCommits: 0,
			};
			for (const member of members) {
				if (!member) {
					row.safety = "unknown";
					continue;
				}
				row.bytes += member.bytes;
				row.changedFiles += member.changedFiles;
				row.unpushedCommits += member.unpushedCommits;
				if (SAFETY_RANK[member.safety] > SAFETY_RANK[row.safety]) {
					row.safety = member.safety;
				}
			}
			return row;
		})
		.sort((a, b) => b.bytes - a.bytes);
}

/** Checked when the list opens: safe to delete, and deleting frees something. */
export function isPreselectedForCleanup(row: CompletedCleanupRow): boolean {
	return row.safety === "safe" && row.bytes > 0;
}

export function selectedCleanupBytes(
	rows: readonly CompletedCleanupRow[],
	selected: ReadonlySet<string>,
): number {
	return rows.reduce(
		(total, row) => (selected.has(row.workspace.id) ? total + row.bytes : total),
		0,
	);
}
