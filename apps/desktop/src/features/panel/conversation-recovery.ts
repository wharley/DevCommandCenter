import type { WorkspaceMessage } from "./thread-projection";

export function buildSafeContinuationPrompt(input: {
	originalPrompt?: string | null;
	preamble: string;
	originalLabel: string;
}) {
	const { originalPrompt, preamble, originalLabel } = input;
	const original = originalPrompt?.trim();
	if (!original) {
		return preamble.trim();
	}
	return `${preamble.trim()}\n\n${originalLabel.trim()}:\n${original}`;
}

/**
 * Changes for every visible streaming update, not only when a new message ID
 * appears. This lets the viewport report new activity while the user is
 * reading older content without forcing their scroll position to move.
 */
export function latestConversationActivitySignature(
	messages: WorkspaceMessage[],
) {
	const latest = messages[messages.length - 1];
	if (!latest) return "empty";
	const visibleActivity = JSON.stringify({
		content: latest.content,
		streaming: latest.streaming,
		status: latest.status,
		annotations: latest.annotations,
	});
	let hash = 2166136261;
	for (let index = 0; index < visibleActivity.length; index += 1) {
		hash ^= visibleActivity.charCodeAt(index);
		hash = Math.imul(hash, 16777619);
	}
	return `${latest.id}:${(hash >>> 0).toString(36)}`;
}

export function precedingUserPrompt(
	messages: WorkspaceMessage[],
	messageIndex: number,
) {
	for (let index = messageIndex - 1; index >= 0; index -= 1) {
		if (messages[index]?.role === "user") {
			return messages[index].content;
		}
	}
	return null;
}
