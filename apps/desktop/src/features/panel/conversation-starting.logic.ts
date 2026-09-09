import type { WorkspaceMessage } from "./thread-projection";

export type ConversationStartingPhase = "creating" | "sending" | "waiting";

export function conversationStartingPhase(
	sessionId: string | null,
	lastTurnState: string | null,
): ConversationStartingPhase {
	if (!sessionId) return "creating";
	return lastTurnState === "running" ? "waiting" : "sending";
}

/** Only the first turn replaces the timeline with the preparation steps. */
export function shouldShowInitialConversationStarting(
	messages: WorkspaceMessage[],
	pendingPrompt: string | null,
	lastTurnState: string | null,
): boolean {
	if (!pendingPrompt?.trim() && lastTurnState !== "running") return false;
	if (messages.filter(message => message.role === "user").length > 1) return false;
	return !messages.some(message =>
		message.role === "assistant" || Boolean(message.delegation) ||
		message.label === "session.aborted" || message.label === "session.completed",
	);
}

/**
 * A persisted conversation already renders an optimistic user message while a
 * turn is being accepted. Keep visible feedback below it until the runtime
 * emits the first real assistant activity.
 */
export function shouldShowConversationStarting(
	messages: WorkspaceMessage[],
	pendingPrompt: string | null,
	lastTurnState: string | null,
): boolean {
	const latestUserIndex = messages.findLastIndex(
		(message) => message.role === "user",
	);
	if (latestUserIndex < 0) return false;

	const latestUserMessage = messages[latestUserIndex];
	const isAwaitingActivity =
		Boolean(pendingPrompt?.trim()) ||
		(lastTurnState === "running" && Boolean(latestUserMessage?.turnId));
	if (!isAwaitingActivity) return false;

	return !messages.slice(latestUserIndex + 1).some(
		(message) => message.role === "assistant" || Boolean(message.delegation),
	);
}
