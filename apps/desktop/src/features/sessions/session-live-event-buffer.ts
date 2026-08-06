import type { CoreEvent } from "@dcc/contracts";

export const SESSION_LIVE_EVENT_LIMIT = 800;
export const SESSION_LIVE_BYTE_LIMIT = 8 * 1024 * 1024;

type SessionBucket = { events: CoreEvent[]; bytes: number };

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

function coalesce(previous: CoreEvent, next: CoreEvent): CoreEvent | null {
	const left = mergeableDelta(previous);
	const right = mergeableDelta(next);
	if (!left || !right || left.type !== right.type || left.key !== right.key) return null;
	return {
		[left.type]: {
			...left.payload,
			...right.payload,
			content: `${left.payload.content as string}${right.payload.content as string}`,
		},
	} as unknown as CoreEvent;
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
			bucket.events[lastIndex] = merged;
			const deltaContent = mergeableDelta(event)?.payload.content;
			bucket.bytes +=
				typeof deltaContent === "string" ? deltaContent.length * 2 : eventBytes(event);
		} else {
			bucket.events.push(event);
			bucket.bytes += eventBytes(event);
		}
		while (
			bucket.events.length > SESSION_LIVE_EVENT_LIMIT ||
			bucket.bytes > SESSION_LIVE_BYTE_LIMIT
		) {
			const removed = bucket.events.shift();
			if (!removed) break;
			bucket.bytes -= eventBytes(removed);
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
		return [...this.unscoped, ...[...this.buckets.values()].flatMap((bucket) => bucket.events)];
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
		const terminalIndex = bucket.events.findIndex(predicate);
		if (terminalIndex < 0) return;
		bucket.events.splice(0, terminalIndex + 1);
		bucket.bytes = bucket.events.reduce((sum, event) => sum + eventBytes(event), 0);
		if (bucket.events.length === 0) this.buckets.delete(sessionId);
	}
}
