import type { WorkspaceMessage } from "./thread-projection";

export const THREAD_FIND_MIN_QUERY_CHARS = 2;
export const THREAD_FIND_SNIPPET_CHARS = 96;
export const THREAD_FIND_MAX_MATCHES = 500;

export type ThreadFindMatch = {
	messageId: string;
	role: WorkspaceMessage["role"];
	/** Bounded excerpt around the first occurrence, for the results readout. */
	snippet: string;
};

function normalizeQuery(query: string) {
	return query.trim().toLocaleLowerCase();
}

/**
 * Renderer-side find over the projected messages of one conversation. Full-text
 * search across sessions stays in the backend FTS; this is the "where was
 * that in this thread" gesture, ordered by timeline position and bounded.
 */
export function findInThread(
	messages: readonly WorkspaceMessage[],
	query: string,
): ThreadFindMatch[] {
	const needle = normalizeQuery(query);
	if (needle.length < THREAD_FIND_MIN_QUERY_CHARS) return [];
	const matches: ThreadFindMatch[] = [];
	for (const message of messages) {
		if (message.streaming === true) continue;
		const haystack = message.content.toLocaleLowerCase();
		const at = haystack.indexOf(needle);
		if (at < 0) continue;
		const start = Math.max(0, at - Math.floor((THREAD_FIND_SNIPPET_CHARS - needle.length) / 2));
		const end = Math.min(message.content.length, start + THREAD_FIND_SNIPPET_CHARS);
		const snippet = `${start > 0 ? "…" : ""}${message.content.slice(start, end).replace(/\s+/g, " ")}${end < message.content.length ? "…" : ""}`;
		matches.push({ messageId: message.id, role: message.role, snippet });
		if (matches.length >= THREAD_FIND_MAX_MATCHES) break;
	}
	return matches;
}

/** Wraps around in both directions; returns 0 for an empty list. */
export function stepThreadFindIndex(
	current: number,
	total: number,
	direction: 1 | -1,
): number {
	if (total <= 0) return 0;
	return (((current + direction) % total) + total) % total;
}
