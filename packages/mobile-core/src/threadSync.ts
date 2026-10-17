/**
 * Delta sync for a thread's durable history. The server's per-session
 * `sequence` is the cursor; `GET .../events?after=<cursor>` returns only what
 * the client is missing, plus `lastSeq` so an empty delta still confirms it.
 */
import type { RawSessionEvent } from "./threadEvents.ts";

export type EventsDelta = {
	events: RawSessionEvent[];
	/** Highest sequence the session has, even when `events` is empty. */
	lastSeq: number;
	hasMore: boolean;
	/** The cursor fell inside merged history; only a full reload is correct. */
	reset?: boolean;
};

export type DeltaPlan =
	| { kind: "apply"; events: RawSessionEvent[]; cursor: number; more: boolean }
	/** The delta cannot be applied on top of what is held: refetch everything. */
	| { kind: "reset"; reason: "history-shrank" | "history-compacted" | "rewind" };

/** A rewind rewrites which turns are visible; the thread folds from scratch. */
export function isRewind(event: RawSessionEvent): boolean {
	return event.kind.type === "conversation_rewound";
}

export function maxSequence(events: RawSessionEvent[], floor = 0): number {
	return events.reduce((max, event) => Math.max(max, event.sequence), floor);
}

export function planDelta(cursor: number, delta: EventsDelta): DeltaPlan {
	if (delta.reset) return { kind: "reset", reason: "history-compacted" };
	// The session's history is behind what we hold (deleted and recreated).
	if (delta.lastSeq < cursor) return { kind: "reset", reason: "history-shrank" };
	const events = delta.events
		.filter((event) => event.sequence > cursor)
		.sort((a, b) => a.sequence - b.sequence);
	if (events.some(isRewind)) return { kind: "reset", reason: "rewind" };
	return {
		kind: "apply",
		events,
		cursor: maxSequence(events, cursor),
		more: delta.hasMore,
	};
}

export function deltaPath(threadId: string, cursor: number): string {
	return `/api/v1/sessions/${encodeURIComponent(threadId)}/events?after=${cursor}&limit=500`;
}
