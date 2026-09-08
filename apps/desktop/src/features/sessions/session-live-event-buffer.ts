import type { CoreEvent } from "@dcc/contracts";

export const SESSION_LIVE_EVENT_LIMIT = 800;
export const SESSION_LIVE_BYTE_LIMIT = 8 * 1024 * 1024;

type BufferedEvent = {
	event: CoreEvent;
	bytes: number;
	contentBytes: number;
	lastCodeUnit: number;
};
type SessionBucket = { events: BufferedEvent[]; bytes: number };

function eventPayload(event: CoreEvent): Record<string, unknown> | null {
	const payload = Object.values(event)[0];
	return payload && typeof payload === "object"
		? (payload as Record<string, unknown>)
		: null;
}

export function sessionIdForLiveEvent(event: CoreEvent): string | null {
	const sessionId = eventPayload(event)?.session_id;
	return typeof sessionId === "string" ? sessionId : null;
}

function eventBytes(event: CoreEvent) {
	return JSON.stringify(event).length * 2;
}

function mergeableDelta(event: CoreEvent) {
	const [type, value] = Object.entries(event)[0] ?? [];
	if (!type?.endsWith("Delta") || !value || typeof value !== "object") return null;
	const payload = value as Record<string, unknown>;
	if (typeof payload.content !== "string") return null;
	return {
		type,
		key: [
			payload.session_id,
			payload.turn_id,
			payload.message_id,
			payload.reasoning_id,
			payload.tool_call_id,
			payload.delegation_id,
		].join(":"),
		payload,
	};
}

function deltaMetadataBytes(type: string, payload: Record<string, unknown>) {
	return eventBytes({ [type]: { ...payload, content: "" } } as unknown as CoreEvent);
}

function serializedContentBytes(content: string) {
	return (JSON.stringify(content).length - 2) * 2;
}

function bufferedEvent(event: CoreEvent): BufferedEvent {
	const delta = mergeableDelta(event);
	if (!delta) return { event, bytes: eventBytes(event), contentBytes: 0, lastCodeUnit: NaN };
	const content = delta.payload.content as string;
	const contentBytes = serializedContentBytes(content);
	return {
		event,
		contentBytes,
		lastCodeUnit: content.charCodeAt(content.length - 1),
		bytes: deltaMetadataBytes(delta.type, delta.payload) + contentBytes,
	};
}

function coalesce(previous: BufferedEvent, next: CoreEvent): BufferedEvent | null {
	const left = mergeableDelta(previous.event);
	const right = mergeableDelta(next);
	if (!left || !right || left.type !== right.type || left.key !== right.key) return null;
	const leftContent = left.payload.content as string;
	const rightContent = right.payload.content as string;
	// Cache the boundary: indexing the growing concatenation can flatten ropes.
	const last = previous.lastCodeUnit;
	const first = rightContent.charCodeAt(0);
	// JSON escapes isolated surrogates as six characters. Joining a split pair
	// replaces two escapes with two UTF-16 code units (20 fewer estimated bytes).
	const joinsSurrogatePair =
		last >= 0xd800 && last <= 0xdbff && first >= 0xdc00 && first <= 0xdfff;
	const contentBytes =
		previous.contentBytes + serializedContentBytes(rightContent) - (joinsSurrogatePair ? 20 : 0);
	const payload = { ...left.payload, ...right.payload, content: leftContent + rightContent };
	return {
		event: { [left.type]: payload } as unknown as CoreEvent,
		contentBytes,
		lastCodeUnit: rightContent.length ? rightContent.charCodeAt(rightContent.length - 1) : last,
		bytes: deltaMetadataBytes(left.type, payload) + contentBytes,
	};
}

/** In-memory transient overlay. SQLite remains the source of durable history. */
export class SessionLiveEventBuffer {
	private readonly buckets = new Map<string, SessionBucket>();
	private readonly unscoped: CoreEvent[] = [];

	append(event: CoreEvent) {
		const sessionId = sessionIdForLiveEvent(event);
		if (!sessionId) {
			this.unscoped.push(event);
			this.unscoped.splice(0, Math.max(0, this.unscoped.length - 32));
			return;
		}
		const bucket = this.buckets.get(sessionId) ?? { events: [], bytes: 0 };
		const lastIndex = bucket.events.length - 1;
		const merged = lastIndex >= 0 ? coalesce(bucket.events[lastIndex], event) : null;
		if (merged) {
			bucket.bytes += merged.bytes - bucket.events[lastIndex].bytes;
			bucket.events[lastIndex] = merged;
		} else {
			const entry = bufferedEvent(event);
			bucket.events.push(entry);
			bucket.bytes += entry.bytes;
		}
		while (
			bucket.events.length > SESSION_LIVE_EVENT_LIMIT ||
			bucket.bytes > SESSION_LIVE_BYTE_LIMIT
		) {
			const removed = bucket.events.shift();
			if (!removed) break;
			// Use the same charge as insertion/merge; never rescan a large removed delta.
			bucket.bytes -= removed.bytes;
		}
		this.buckets.set(sessionId, bucket);
	}

	purgeSession(sessionId: string) {
		this.buckets.delete(sessionId);
	}

	purgeThroughTurn(sessionId: string, turnId: string) {
		this.purgeThrough(sessionId, (event) => {
			const terminal =
				("sessionTurnCompleted" in event && event.sessionTurnCompleted) ||
				("sessionTurnAborted" in event && event.sessionTurnAborted) ||
				null;
			return terminal?.turn_id === turnId;
		});
	}

	purgeThroughSessionTerminal(sessionId: string) {
		this.purgeThrough(
			sessionId,
			(event) =>
				("sessionCompleted" in event && Boolean(event.sessionCompleted)) ||
				("sessionAborted" in event && Boolean(event.sessionAborted)),
		);
	}

	purgeSessions(sessionIds: Iterable<string>) {
		for (const sessionId of sessionIds) this.buckets.delete(sessionId);
	}

	events() {
		return [
			...this.unscoped,
			...[...this.buckets.values()].flatMap((bucket) => bucket.events.map((entry) => entry.event)),
		];
	}

	eventsForSession(sessionId: string | null) {
		if (!sessionId) return [...this.unscoped];
		return [
			...this.unscoped,
			...(this.buckets.get(sessionId)?.events.map((entry) => entry.event) ?? []),
		];
	}

	stats() {
		return [...this.buckets.entries()].map(([sessionId, bucket]) => ({
			sessionId,
			events: bucket.events.length,
			bytes: bucket.bytes,
		}));
	}

	private purgeThrough(sessionId: string, predicate: (event: CoreEvent) => boolean) {
		const bucket = this.buckets.get(sessionId);
		if (!bucket) return;
		const terminalIndex = bucket.events.findIndex((entry) => predicate(entry.event));
		if (terminalIndex < 0) return;
		bucket.events.splice(0, terminalIndex + 1);
		bucket.bytes = bucket.events.reduce((sum, entry) => sum + entry.bytes, 0);
		if (bucket.events.length === 0) this.buckets.delete(sessionId);
	}
}
