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

const MIN_REVIEW_CARD_DIFF_HEIGHT = 190;
const MAX_REVIEW_CARD_DIFF_HEIGHT = 520;
const REVIEW_CARD_LINE_HEIGHT = 18;

/**
 * Keeps small diffs readable while preventing a single large file from taking
 * over the continuous review feed. Large cards remain independently scrollable.
 */
export function reviewCardDiffHeight(
	insertions: number,
	deletions: number,
): number {
	const changedLines = Math.max(0, insertions) + Math.max(0, deletions);
	return Math.min(
		MAX_REVIEW_CARD_DIFF_HEIGHT,
		Math.max(
			MIN_REVIEW_CARD_DIFF_HEIGHT,
			MIN_REVIEW_CARD_DIFF_HEIGHT + changedLines * REVIEW_CARD_LINE_HEIGHT,
		),
	);
}

/** The first cards render immediately; the remainder are loaded near viewport. */
export function shouldEagerLoadReviewCard(index: number): boolean {
	return index >= 0 && index < 2;
}
