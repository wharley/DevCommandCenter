import type { WorkspaceMessage } from "./thread-projection";

export const INITIAL_CONVERSATION_MESSAGE_LIMIT = 80;
export const CONVERSATION_MESSAGE_PAGE_SIZE = 80;

/**
 * Keeps the recent transcript mounted while avoiding a partial user/assistant
 * turn at the top of the window. Older messages remain available from SQLite
 * and can be progressively revealed by increasing `messageLimit`.
 */
export function conversationWindowStart(
	messages: readonly WorkspaceMessage[],
	messageLimit: number,
): number {
	if (messages.length <= messageLimit) return 0;

	let start = Math.max(0, messages.length - Math.max(1, messageLimit));
	while (start > 0 && messages[start]?.role !== "user") {
		start -= 1;
	}
	return start;
}
