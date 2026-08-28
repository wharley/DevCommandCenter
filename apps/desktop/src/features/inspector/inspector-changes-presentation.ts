export type InspectorChangesScope = "working" | "branch";
export type InspectorChangeGroup = "staged" | "unstaged" | "committed";

type ChangeStatsEntry = {
	insertions: number;
	deletions: number;
};

export type InspectorChangesSummary = {
	fileCount: number;
	insertions: number;
	deletions: number;
};

export function summarizeInspectorChanges(
	entries: readonly ChangeStatsEntry[],
): InspectorChangesSummary {
	return entries.reduce<InspectorChangesSummary>(
		(summary, entry) => ({
			fileCount: summary.fileCount + 1,
			insertions: summary.insertions + Math.max(0, entry.insertions),
			deletions: summary.deletions + Math.max(0, entry.deletions),
		}),
		{ fileCount: 0, insertions: 0, deletions: 0 },
	);
}

export function defaultInspectorChangesScope(
	workingFileCount: number,
	branchFileCount: number,
): InspectorChangesScope {
	if (workingFileCount > 0 || branchFileCount === 0) {
		return "working";
	}
	return "branch";
}

export function changeGroupBelongsToScope(
	group: InspectorChangeGroup,
	scope: InspectorChangesScope,
): boolean {
	return scope === "branch" ? group === "committed" : group !== "committed";
}
