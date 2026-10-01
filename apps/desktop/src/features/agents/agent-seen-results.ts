import { useSyncExternalStore } from "react";

/**
 * Which finished agent results the person has already looked at, per session.
 * Kept in this browser profile: it is a reading marker, not task state.
 */
const STORAGE_KEY = "dcc.agents.seenResults";
const MAX_ENTRIES = 300;

type SeenResults = Record<string, string>;

const listeners = new Set<() => void>();
let cache: SeenResults | null = null;

function read(): SeenResults {
	if (cache) {
		return cache;
	}
	try {
		const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
		cache = parsed && typeof parsed === "object" ? (parsed as SeenResults) : {};
	} catch {
		cache = {};
	}
	return cache;
}

/** A result is unread until the person has seen a turn finished at or after it. */
export function isResultUnread(
	completedAt: string | null | undefined,
	seenAt: string | undefined,
): boolean {
	return Boolean(completedAt) && (!seenAt || seenAt < (completedAt as string));
}

/** Keeps the newest entries so the marker cannot grow without bound. */
export function withSeenResult(
	seen: SeenResults,
	sessionId: string,
	completedAt: string,
	maxEntries = MAX_ENTRIES,
): SeenResults {
	const entries = Object.entries({ ...seen, [sessionId]: completedAt })
		.sort(([, left], [, right]) => right.localeCompare(left))
		.slice(0, maxEntries);
	return Object.fromEntries(entries);
}

export function markAgentResultSeen(sessionId: string, completedAt: string): void {
	const seen = read();
	if (!isResultUnread(completedAt, seen[sessionId])) {
		return;
	}
	cache = withSeenResult(seen, sessionId, completedAt);
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(cache));
	} catch {
		// Storage can be unavailable or full; the marker then lasts for this run.
	}
	for (const listener of listeners) {
		listener();
	}
}

export function useSeenAgentResults(): SeenResults {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		read,
		read,
	);
}
