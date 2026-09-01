import type {
	CoreEvent,
	SessionEventRecord,
	SessionLiveEventEnvelope,
	SessionLiveSnapshot,
} from "@dcc/contracts";

import { sessionIdForLiveEvent } from "./session-live-event-buffer";

export const SESSION_LIVE_RECONCILE_BUFFER_LIMIT = 512;
export const SESSION_LIVE_RECONCILE_DEDUPE_LIMIT = 2048;
export const SESSION_LIVE_RECONCILE_BYTE_LIMIT = 8 * 1024 * 1024;
const MAX_SAFE_SEQUENCE = Number.MAX_SAFE_INTEGER;
const MAX_RUNTIME_GENERATION_LENGTH = 128;
const MAX_EVENT_ID_LENGTH = 512;

export type SessionLiveReconcileState = {
	sessionId: string;
	history: SessionEventRecord[];
	liveEvents: CoreEvent[];
	ready: boolean;
};

export type SessionLiveReconcileResult = {
	changed: boolean;
	rehydrate: boolean;
	state: SessionLiveReconcileState;
};

function emptyState(sessionId: string): SessionLiveReconcileState {
	return { sessionId, history: [], liveEvents: [], ready: false };
}

function liveKey(envelope: SessionLiveEventEnvelope) {
	return tupleKey(envelope.runtimeGeneration, envelope.runtimeSequence);
}

function durableKey(envelope: SessionLiveEventEnvelope) {
	const durable = envelope.durable;
	return durable
		? tupleKey(durable.sessionId, durable.eventId, durable.sequence)
		: null;
}

function tupleKey(...parts: Array<string | number>) {
	return parts
		.map((part) => {
			const value = String(part);
			return `${value.length}:${value}`;
		})
		.join("|");
}

/**
 * Bounded, session-scoped reconciliation for the additive live transport.
 * Durable history remains the authority; this object keeps only an in-memory
 * overlay published after the last snapshot high-water mark.
 */
export class SessionLiveReconciler {
	private state: SessionLiveReconcileState;
	private runtimeGeneration: string | null = null;
	private highWatermark = 0;
	private hydrating = true;
	private buffer: SessionLiveEventEnvelope[] = [];
	private bufferBytes = 0;
	private readonly seenLive = new Set<string>();
	private readonly seenDurable = new Set<string>();
	private readonly durableEventIdBySequence = new Map<number, string>();
	private pendingRuntimeGeneration: string | null = null;
	private disposed = false;
	private liveOverlay: SessionLiveEventEnvelope[] = [];
	private liveOverlayBytes = 0;

	constructor(private readonly sessionId: string) {
		this.state = emptyState(sessionId);
	}

	current() {
		return this.state;
	}

	dispose() {
		this.disposed = true;
		this.buffer = [];
		this.bufferBytes = 0;
		this.seenLive.clear();
		this.seenDurable.clear();
		this.durableEventIdBySequence.clear();
		this.liveOverlay = [];
		this.liveOverlayBytes = 0;
	}

	beginHydration(): SessionLiveReconcileResult {
		this.hydrating = true;
		this.state = { ...this.state, ready: false };
		return this.result(true, false);
	}

	acceptEnvelope(envelope: SessionLiveEventEnvelope): SessionLiveReconcileResult {
		if (this.disposed) return this.result(false, false);
		if (!this.validEnvelope(envelope)) {
			this.resetForRehydrate();
			return this.result(true, true);
		}
		if (sessionIdForLiveEvent(envelope.event) !== this.sessionId) {
			return this.result(false, false);
		}

		if (this.runtimeGeneration !== null && envelope.runtimeGeneration !== this.runtimeGeneration) {
			this.resetForNewGeneration(envelope.runtimeGeneration);
			this.bufferEnvelope(envelope);
			return this.result(true, true);
		}

		if (this.hydrating) {
			this.expectRuntimeGeneration(envelope.runtimeGeneration);
			return this.bufferEnvelope(envelope);
		}

		return this.applyEnvelope(envelope);
	}

	acceptSnapshot(snapshot: SessionLiveSnapshot): SessionLiveReconcileResult {
		if (this.disposed) return this.result(false, false);
		if (!snapshot || snapshot.sessionId !== this.sessionId) {
			return this.result(false, false);
		}
		if (!this.validSnapshot(snapshot)) {
			this.resetForRehydrate();
			return this.result(true, true);
		}

		if (
			this.pendingRuntimeGeneration !== null &&
			snapshot.runtimeGeneration !== this.pendingRuntimeGeneration
		) {
			// An oversized or overflowed live envelope may have been discarded,
			// but its generation is still an authoritative stale-snapshot guard.
			return this.result(false, true);
		}

		this.runtimeGeneration = snapshot.runtimeGeneration;
		this.pendingRuntimeGeneration = null;
		this.highWatermark = snapshot.durableHighWatermark;
		this.hydrating = false;
		this.seenLive.clear();
		this.seenDurable.clear();
		this.durableEventIdBySequence.clear();
		this.state = {
			sessionId: this.sessionId,
			history: snapshot.events,
			liveEvents: [],
			ready: true,
		};

		const buffered = this.buffer;
		this.buffer = [];
		this.bufferBytes = 0;
		let changed = true;
		for (const envelope of buffered) {
			const result = this.applyEnvelope(envelope);
			changed ||= result.changed;
			if (result.rehydrate) return this.result(changed, true);
		}
		return this.result(changed, false);
	}

	private applyEnvelope(envelope: SessionLiveEventEnvelope): SessionLiveReconcileResult {
		if (envelope.runtimeGeneration !== this.runtimeGeneration) {
			this.resetForNewGeneration(envelope.runtimeGeneration);
			this.bufferEnvelope(envelope);
			return this.result(true, true);
		}

		const key = liveKey(envelope);
		if (this.seenLive.has(key)) return this.result(false, false);
		if (this.seenLive.size >= SESSION_LIVE_RECONCILE_DEDUPE_LIMIT) {
			this.resetForRehydrate(envelope);
			return this.result(true, true);
		}
		this.seenLive.add(key);

		const durableEnvelope = envelope.durable;
		const identity = durableKey(envelope);
		if (identity) {
			if (this.seenDurable.has(identity)) return this.result(false, false);
			const knownEventId = this.durableEventIdBySequence.get(durableEnvelope!.sequence);
			if (knownEventId && knownEventId !== durableEnvelope!.eventId) {
				this.resetForRehydrate(envelope);
				return this.result(true, true);
			}
			if (this.seenDurable.size >= SESSION_LIVE_RECONCILE_DEDUPE_LIMIT) {
				this.resetForRehydrate(envelope);
				return this.result(true, true);
			}
			this.seenDurable.add(identity);
			this.durableEventIdBySequence.set(
				durableEnvelope!.sequence,
				durableEnvelope!.eventId,
			);
			if (durableEnvelope && durableEnvelope.sequence <= this.highWatermark) {
				return this.result(false, false);
			}
		}

		if (this.liveOverlay.length >= SESSION_LIVE_RECONCILE_BUFFER_LIMIT) {
			this.resetForRehydrate(envelope);
			return this.result(true, true);
		}
		const bytes = envelopeBytes(envelope);
		if (this.liveOverlayBytes + bytes > SESSION_LIVE_RECONCILE_BYTE_LIMIT) {
			this.resetForRehydrate(envelope);
			return this.result(true, true);
		}
		this.liveOverlay.push(envelope);
		this.liveOverlayBytes += bytes;
		const orderedDurable = this.liveOverlay
			.filter((entry) => entry.durable)
			.sort((left, right) => left.durable!.sequence - right.durable!.sequence);
		let durableIndex = 0;
		this.state = {
			...this.state,
			// Keep ephemeral slots in native arrival order. Only durable records
			// are reordered, by their canonical SQLite sequence, giving a total
			// and transitive order without changing runtime-only status timing.
			liveEvents: this.liveOverlay.map((entry) =>
				entry.durable ? orderedDurable[durableIndex++]!.event : entry.event,
			),
		};
		return this.result(true, false);
	}

	private bufferEnvelope(envelope: SessionLiveEventEnvelope): SessionLiveReconcileResult {
		const bytes = envelopeBytes(envelope);
		if (
			this.buffer.length >= SESSION_LIVE_RECONCILE_BUFFER_LIMIT ||
			this.bufferBytes + bytes > SESSION_LIVE_RECONCILE_BYTE_LIMIT
		) {
			this.resetForRehydrate(envelope);
			return this.result(true, true);
		}
		this.buffer.push(envelope);
		this.bufferBytes += bytes;
		return this.result(false, false);
	}

	private expectRuntimeGeneration(runtimeGeneration: string) {
		if (this.pendingRuntimeGeneration === runtimeGeneration) return;
		if (this.pendingRuntimeGeneration !== null) {
			this.resetForNewGeneration(runtimeGeneration);
			return;
		}
		this.pendingRuntimeGeneration = runtimeGeneration;
	}

	private resetForNewGeneration(runtimeGeneration: string) {
		this.runtimeGeneration = null;
		this.pendingRuntimeGeneration = runtimeGeneration;
		this.highWatermark = 0;
		this.hydrating = true;
		this.buffer = [];
		this.bufferBytes = 0;
		this.seenLive.clear();
		this.seenDurable.clear();
		this.durableEventIdBySequence.clear();
		this.liveOverlay = [];
		this.liveOverlayBytes = 0;
		this.state = emptyState(this.sessionId);
	}

	private resetForRehydrate(envelope?: SessionLiveEventEnvelope) {
		this.hydrating = true;
		const bytes = envelope ? envelopeBytes(envelope) : 0;
		this.buffer = envelope && bytes <= SESSION_LIVE_RECONCILE_BYTE_LIMIT ? [envelope] : [];
		this.bufferBytes = this.buffer.length === 1 ? bytes : 0;
		this.seenLive.clear();
		this.seenDurable.clear();
		this.durableEventIdBySequence.clear();
		this.liveOverlay = [];
		this.liveOverlayBytes = 0;
		this.state = emptyState(this.sessionId);
	}

	private validEnvelope(envelope: SessionLiveEventEnvelope) {
		try {
			if (
				!envelope ||
				typeof envelope.runtimeGeneration !== "string" ||
				envelope.runtimeGeneration.trim().length === 0 ||
				envelope.runtimeGeneration.length > MAX_RUNTIME_GENERATION_LENGTH ||
				!Number.isSafeInteger(envelope.runtimeSequence) ||
				envelope.runtimeSequence <= 0 ||
				!envelope.event ||
				typeof envelope.event !== "object"
			) {
				return false;
			}
			const sessionId = sessionIdForLiveEvent(envelope.event);
			if (!sessionId) return false;
			const durable = envelope.durable;
			return (
				!durable ||
				(typeof durable.sessionId === "string" &&
					durable.sessionId === sessionId &&
					typeof durable.eventId === "string" &&
					durable.eventId.trim().length > 0 &&
					durable.eventId.length <= MAX_EVENT_ID_LENGTH &&
					Number.isSafeInteger(durable.sequence) &&
					durable.sequence > 0 &&
					durable.sequence <= MAX_SAFE_SEQUENCE)
			);
		} catch {
			return false;
		}
	}

	private validSnapshot(snapshot: SessionLiveSnapshot) {
		if (
			snapshot.sessionId !== this.sessionId ||
			typeof snapshot.runtimeGeneration !== "string" ||
			snapshot.runtimeGeneration.trim().length === 0 ||
			snapshot.runtimeGeneration.length > MAX_RUNTIME_GENERATION_LENGTH ||
			!Number.isSafeInteger(snapshot.durableHighWatermark) ||
			snapshot.durableHighWatermark < 0 ||
			snapshot.durableHighWatermark > MAX_SAFE_SEQUENCE
		) {
			return false;
		}
		if (!Array.isArray(snapshot.events)) return false;
		let previous = 0;
		for (const record of snapshot.events) {
			if (
				!record ||
				record.sessionId !== this.sessionId ||
				typeof record.eventId !== "string" ||
				record.eventId.trim().length === 0 ||
				record.eventId.length > MAX_EVENT_ID_LENGTH ||
				!Number.isSafeInteger(record.sequence) ||
				record.sequence <= previous ||
				record.sequence > MAX_SAFE_SEQUENCE
			) {
				return false;
			}
			previous = record.sequence;
		}
		return snapshot.durableHighWatermark === previous;
	}

	private result(changed: boolean, rehydrate: boolean): SessionLiveReconcileResult {
		return { changed, rehydrate, state: this.state };
	}
}

function envelopeBytes(envelope: SessionLiveEventEnvelope) {
	try {
		return JSON.stringify(envelope).length * 2;
	} catch {
		return SESSION_LIVE_RECONCILE_BYTE_LIMIT + 1;
	}
}
