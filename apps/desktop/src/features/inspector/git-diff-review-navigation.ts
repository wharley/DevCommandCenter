/** True while a changed file is using the focused Inspector review pane. */
export function isInlineGitDiffReview(
	surfaceKind: string | null,
	gitDiffExpanded: boolean,
) {
	return surfaceKind === "git-diff" && !gitDiffExpanded;
}

/**
 * Expanded review is one navigation level above the changed-files list.
 * Dismissing that level must return to the list instead of closing Review.
 */
export function shouldReturnToGitFiles(
	surfaceKind: string | null,
	gitDiffExpanded: boolean,
) {
	return surfaceKind === "git-diff" && gitDiffExpanded;
}
