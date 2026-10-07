import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

export type AssistantActivityAnnotation = Extract<
	WorkspaceMessageAnnotation,
	{ type: "commentary" | "reasoning" | "tool-call" }
>;

export function isActivityAnnotation(
	annotation: WorkspaceMessageAnnotation,
): annotation is AssistantActivityAnnotation {
	return (
		annotation.type === "commentary" ||
		annotation.type === "reasoning" ||
		annotation.type === "tool-call"
	);
}
