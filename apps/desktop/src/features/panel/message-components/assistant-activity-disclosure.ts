import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

export function shouldAutoOpenAssistantActivity(
	annotations: WorkspaceMessageAnnotation[],
) {
	return annotations.some(
		(annotation) =>
			Boolean(annotation.streaming) ||
			(annotation.type === "tool-call" && annotation.status?.type === "failed"),
	);
}
