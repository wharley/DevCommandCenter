import type {
	CoreEvent,
	SessionEventRecord,
	SessionLiveEventEnvelope,
	SessionLiveSnapshot,
} from "@dcc/contracts";
import { describe, expect, it } from "vitest";

import {
	SESSION_LIVE_RECONCILE_BYTE_LIMIT,
	SESSION_LIVE_RECONCILE_BUFFER_LIMIT,
	SessionLiveReconciler,
} from "./session-live-reconciler";

function event(sessionId: string, label: string): CoreEvent {
	return {
		sessionTurnDelta: {
			session_id: sessionId,
			turn_id: "turn-1",
			content: label,
		},
	} as CoreEvent;
}

function envelope(
	sessionId: string,
	generation: string,
	sequence: number,
	durableSequence: number | null = null,
): SessionLiveEventEnvelope {
	return {
		runtimeGeneration: generation,
		runtimeSequence: sequence,
		durable:
			durableSequence === null
				? null
				: {
					sessionId,
					eventId: `event-${durableSequence}`,
					sequence: durableSequence,
				},
		event: event(sessionId, `event-${sequence}`),
	} as SessionLiveEventEnvelope;
}

function record(sessionId: string, sequence: number): SessionEventRecord {
	return {
		eventId: `event-${sequence}`,
		sessionId,
		sequence,
		occurredAt: "2026-09-01T00:00:00Z",
		kind: {
			type: "turn_delta",
			turnId: "turn-1",
			content: "persisted",
		},
	};
}

function snapshot(
	sessionId: string,
	generation: string,
	highWatermark = 1,
): SessionLiveSnapshot {
	return {
		sessionId,
		events: highWatermark ? [record(sessionId, highWatermark)] : [],
		durableHighWatermark: highWatermark,
		runtimeGeneration: generation,
	};
}

describe("SessionLiveReconciler", () => {
	it("buffers an event received after subscribe and applies it after the snapshot", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 1, 2));
		const result = reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1));

		expect(result.rehydrate).toBe(false);
		expect(result.state.history).toHaveLength(1);
		expect(result.state.liveEvents).toHaveLength(1);
	});

	it("deduplicates runtime envelopes and discards durable events at the snapshot watermark", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 4));
		const alreadyHydrated = envelope("session-a", "generation-a", 1, 4);
		reconciler.acceptEnvelope(alreadyHydrated);
		reconciler.acceptEnvelope(alreadyHydrated);
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 2, 5));

		expect(reconciler.current().liveEvents).toHaveLength(1);
	});

	it("orders post-snapshot durable events by canonical sequence despite arrival order", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 2, 3));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 1, 2));

		expect(
			reconciler.current().liveEvents.map((entry) =>
				(entry as { sessionTurnDelta: { content: string } }).sessionTurnDelta.content,
			),
		).toEqual(["event-1", "event-2"]);
	});

	it("keeps ephemeral arrival slots while sorting only the durable subsequence", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 1, 3));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 2));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 3, 2));

		expect(
			reconciler.current().liveEvents.map((entry) =>
				(entry as { sessionTurnDelta: { content: string } }).sessionTurnDelta.content,
			),
		).toEqual(["event-3", "event-2", "event-1"]);
	});

	it("rehydrates on a new runtime generation and accepts only its next snapshot", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a"));
		const switched = reconciler.acceptEnvelope(
			envelope("session-a", "generation-b", 1, 2),
		);
		expect(switched.rehydrate).toBe(true);
		expect(switched.state.ready).toBe(false);

		// A late old-generation snapshot requests one more read instead of
		// applying stale data; the current-generation snapshot then succeeds.
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-a")).rehydrate).toBe(
			true,
		);
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-b")).rehydrate).toBe(
			false,
		);
	});

	it("rejects a stale snapshot and cross-session envelopes", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		expect(reconciler.acceptSnapshot(snapshot("session-b", "generation-b")).changed).toBe(
			false,
		);
		expect(
			reconciler.acceptEnvelope(envelope("session-b", "generation-b", 1, 1)).changed,
		).toBe(false);
		expect(reconciler.current().history).toEqual([]);
	});

	it("ignores late callbacks after unmount disposal", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.dispose();
		expect(
			reconciler.acceptEnvelope(envelope("session-a", "generation-a", 1, 1)).changed,
		).toBe(false);
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-a")).changed).toBe(
			false,
		);
	});

	it("forces rehydration rather than evicting an overflowing pre-snapshot buffer", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		for (let index = 0; index < SESSION_LIVE_RECONCILE_BUFFER_LIMIT; index += 1) {
			expect(
				reconciler.acceptEnvelope(envelope("session-a", "generation-a", index + 1))
					.rehydrate,
			).toBe(false);
		}
		expect(
			reconciler.acceptEnvelope(
				envelope("session-a", "generation-a", SESSION_LIVE_RECONCILE_BUFFER_LIMIT + 1),
			).rehydrate,
		).toBe(true);
	});

	it("fails closed on malformed envelope or incoherent snapshot", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		expect(reconciler.acceptEnvelope({} as SessionLiveEventEnvelope).rehydrate).toBe(true);
		const malformed = envelope("session-a", "", 0, 1);
		expect(reconciler.acceptEnvelope(malformed).rehydrate).toBe(true);
		expect(
			reconciler.acceptSnapshot({
				...snapshot("session-a", "generation-a", 1),
				durableHighWatermark: 2,
			}).rehydrate,
		).toBe(true);
	});

	it("does not retain an envelope that exceeds the byte budget by itself", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		const oversized = envelope("session-a", "generation-a", 1, 2);
		oversized.event = event("session-a", "x".repeat(SESSION_LIVE_RECONCILE_BYTE_LIMIT));
		expect(reconciler.acceptEnvelope(oversized).rehydrate).toBe(true);
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1)).rehydrate).toBe(
			false,
		);
		expect(reconciler.current().liveEvents).toEqual([]);
	});

	it("keeps a pending generation guard when a newer envelope is too large to buffer", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1));
		const oversized = envelope("session-a", "generation-b", 1, 2);
		oversized.event = event("session-a", "x".repeat(SESSION_LIVE_RECONCILE_BYTE_LIMIT));
		expect(reconciler.acceptEnvelope(oversized).rehydrate).toBe(true);

		// The buffer is empty, but the observed new generation still prevents a
		// late old-generation snapshot from replacing the reset state.
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1)).rehydrate).toBe(
			true,
		);
		expect(reconciler.acceptSnapshot(snapshot("session-a", "generation-b", 1)).rehydrate).toBe(
			false,
		);
	});

	it("rejects conflicting durable event IDs for one canonical sequence", () => {
		const reconciler = new SessionLiveReconciler("session-a");
		reconciler.acceptSnapshot(snapshot("session-a", "generation-a", 1));
		reconciler.acceptEnvelope(envelope("session-a", "generation-a", 1, 2));
		const conflict = envelope("session-a", "generation-a", 2, 2);
		conflict.durable!.eventId = "different-event";
		expect(reconciler.acceptEnvelope(conflict).rehydrate).toBe(true);
	});
});
