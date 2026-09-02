import type { WorkspaceMessage } from "./session-thread-history.logic";

/**
 * Fork by message: the person picks one of their own messages and starts a
 * new thread whose bounded re-anchor covers only what came before it. The
 * forked message itself is offered back in the composer for editing, so the
 * new thread never sends the old prompt silently.
 */
export type ForkPoint = {
	/** Durable messages strictly before the forked message, in order. */
	priorMessages: WorkspaceMessage[];
	/** The forked user prompt, offered as an editable draft. */
	forkedPrompt: string;
	/** Count of user turns excluded (the forked one and everything after). */
	excludedUserTurns: number;
};

export function selectForkPoint(
	messages: readonly WorkspaceMessage[],
	messageId: string,
): ForkPoint | null {
	const index = messages.findIndex((message) => message.id === messageId);
	if (index < 0) return null;
	const target = messages[index];
	if (!target || target.role !== "user") return null;
	const priorMessages = messages
		.slice(0, index)
		.filter((message) => message.streaming !== true && !message.status);
	const excludedUserTurns = messages
		.slice(index)
		.filter((message) => message.role === "user").length;
	return { priorMessages, forkedPrompt: target.content, excludedUserTurns };
}

/**
 * Pending fork re-anchors are keyed by the new session and consumed exactly
 * once, by the first turn the person actually sends there.
 */
export class PendingForkReanchors {
	private readonly bySession = new Map<string, string>();

	set(sessionId: string, reanchor: string) {
		this.bySession.set(sessionId, reanchor);
	}

	peek(sessionId: string | null | undefined) {
		return sessionId ? (this.bySession.get(sessionId) ?? null) : null;
	}

	consume(sessionId: string | null | undefined) {
		if (!sessionId) return null;
		const value = this.bySession.get(sessionId) ?? null;
		this.bySession.delete(sessionId);
		return value;
	}

	clear() {
		this.bySession.clear();
	}
}
