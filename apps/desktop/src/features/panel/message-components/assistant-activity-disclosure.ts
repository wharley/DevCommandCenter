import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

export const ASSISTANT_ACTIVITY_AUTO_COLLAPSE_DELAY_MS = 400;

export function isProminentAssistantActivity(
	annotation: WorkspaceMessageAnnotation,
) {
	return (
		Boolean(annotation.streaming) ||
		(annotation.type === "tool-call" && annotation.status?.type === "failed")
	);
}

export function partitionAssistantActivity(
	annotations: WorkspaceMessageAnnotation[],
) {
	return annotations.reduce(
		(partition, annotation, index) => {
			if (isProminentAssistantActivity(annotation)) {
				partition.prominentIndexes.push(index);
			} else {
				partition.historyIndexes.push(index);
			}
			return partition;
		},
		{
			historyIndexes: [] as number[],
			prominentIndexes: [] as number[],
		},
	);
}

export function shouldAutoOpenAssistantActivity(
	annotations: WorkspaceMessageAnnotation[],
	turnStreaming = false,
) {
	return turnStreaming || annotations.some(isProminentAssistantActivity);
}
