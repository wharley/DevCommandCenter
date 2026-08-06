import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

export function shouldAutoOpenAssistantActivity(
	_annotations: WorkspaceMessageAnnotation[],
) {
	// Live state, counters and failures remain visible in the summary. Details
	// only open when the user explicitly asks to inspect the activity timeline.
	return false;
}
