import type { WorkspaceMessageAnnotation } from "../../sessions/session-thread-history.logic";

export const ASSISTANT_ACTIVITY_PAGE_SIZE = 20;
export type AssistantActivityAnnotation = Extract<
	WorkspaceMessageAnnotation,
	{ type: "commentary" | "reasoning" | "tool-call" }
>;
export type ActivityFilter = "all" | "tools" | "updates" | "failures";
export type ActivityActionKind =
	| "command"
	| "files"
	| "research"
	| "planning";

export function classifyAssistantActivityAction(
	action: string,
): ActivityActionKind | null {
	const normalized = action.toLowerCase().replace(/[^a-z0-9]/g, "");
	if (
		["bash", "shell", "commandexecution", "execcommand", "terminal"].includes(
			normalized,
		)
	)
		return "command";
	if (
		["applypatch", "filechange", "write", "writefile", "edit"].includes(
			normalized,
		)
	)
		return "files";
	if (
		["read", "readfile", "glob", "grep", "searchfiles"].includes(normalized)
	)
		return "files";
	if (["websearch", "webfetch", "browser"].includes(normalized))
		return "research";
	if (["todowrite", "updateplan"].includes(normalized)) return "planning";
	return null;
}

export function isActivityAnnotation(
	annotation: WorkspaceMessageAnnotation,
): annotation is AssistantActivityAnnotation {
	return (
		annotation.type === "commentary" ||
		annotation.type === "reasoning" ||
		annotation.type === "tool-call"
	);
}

export function summarizeAssistantActivity(
	annotations: readonly AssistantActivityAnnotation[],
	turnStreaming?: boolean,
	interrupted = false,
	waitingForInput = false,
) {
	// Explicit turn completion wins over stale streaming flags restored from history.
	const live =
		!interrupted &&
		(turnStreaming ?? annotations.some((item) => item.streaming));
	const latest = annotations.at(-1);
	const failures = annotations.filter(
		(item) => item.type === "tool-call" && item.status?.type === "failed",
	).length;
	return {
		live,
		latest,
		failures,
		tools: annotations.filter((item) => item.type === "tool-call").length,
		updates: annotations.filter((item) => item.type !== "tool-call").length,
		state: interrupted
			? "interrupted"
			: live && waitingForInput
				? "waiting"
				: live
					? "running"
					: "history",
	} as const;
}

export function selectAssistantActivity(
	annotations: readonly AssistantActivityAnnotation[],
	filter: ActivityFilter,
	limit = ASSISTANT_ACTIVITY_PAGE_SIZE,
) {
	const matching = annotations
		.map((annotation, index) => ({ annotation, index }))
		.filter(
			({ annotation }) =>
				filter === "all" ||
				(filter === "tools" && annotation.type === "tool-call") ||
				(filter === "updates" && annotation.type !== "tool-call") ||
				(filter === "failures" &&
					annotation.type === "tool-call" &&
					annotation.status?.type === "failed"),
		);
	const entries = matching.slice(-Math.max(1, limit));
	return {
		entries,
		total: matching.length,
		hidden: matching.length - entries.length,
	};
}
