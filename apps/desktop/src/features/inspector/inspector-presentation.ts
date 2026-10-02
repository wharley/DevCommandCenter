export type InspectorPresentation = "contextual" | "pinned";

export function shouldCollapseContextualInspector(
	presentation: InspectorPresentation,
	collapsed: boolean,
): boolean {
	return presentation === "contextual" && !collapsed;
}

/**
 * A merged change request only means "this task is done" when the task owns its
 * branch. A task running directly in the project checkout sits on a long-lived
 * branch (e.g. `develop`), whose latest change request is usually an old merged
 * release, unrelated to the task.
 */
export function shouldAutoCompleteMergedWorkspace(input: {
	commitMode: string;
	workspaceStatus: string | null;
	hasOwnWorktree: boolean;
}): boolean {
	if (input.workspaceStatus === "completed" || input.workspaceStatus === "archived") {
		return false;
	}
	return input.commitMode === "merged" && input.hasOwnWorktree;
}
