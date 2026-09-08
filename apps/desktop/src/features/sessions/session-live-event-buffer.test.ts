import type { CoreEvent } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { SESSION_LIVE_BYTE_LIMIT, SessionLiveEventBuffer } from "./session-live-event-buffer";

const event = (value: object) => value as CoreEvent;

describe("SessionLiveEventBuffer", () => {
	it("accounts for Unicode surrogate pairs split between chunks", () => {
		const buffer = new SessionLiveEventBuffer();
		for (const content of ["ação ", "\ud83e", "", "\udd80", "\ud83e", "x", "\udd80"]) {
			buffer.append(event({ sessionTurnDelta: { session_id: "a", turn_id: "t", content } }));
			expect(buffer.stats()[0].bytes).toBe(JSON.stringify(buffer.events()[0]).length * 2);
		}
	});

	it("keeps byte accounting exact across escaped deltas, eviction and metadata changes", () => {
		const buffer = new SessionLiveEventBuffer();
		for (let index = 0; index < 30; index += 1) {
			buffer.append(event({ sessionTurnDelta: {
				session_id: "a", turn_id: "t", content: "\n\"\\".repeat(100_000),
				sequence: index, note: "metadata".repeat(index % 3),
			} }));
			const actualBytes = buffer.eventsForSession("a")
				.reduce((sum, entry) => sum + JSON.stringify(entry).length * 2, 0);
			expect(buffer.stats()[0]?.bytes ?? 0).toBe(actualBytes);
			expect(actualBytes).toBeLessThanOrEqual(SESSION_LIVE_BYTE_LIMIT);
		}
	});

	it("coalesces deltas independently for parallel sessions", () => {
		const buffer = new SessionLiveEventBuffer();
		for (let index = 0; index < 1000; index += 1) {
			for (const sessionId of ["a", "b"]) {
				buffer.append(
					event({
						sessionTurnDelta: {
							session_id: sessionId,
							turn_id: "t",
							content: "x",
						},
					}),
				);
			}
		}
		const events = buffer.events();
		expect(events).toHaveLength(2);
		expect(
			(events[0] as { sessionTurnDelta: { content: string } }).sessionTurnDelta
				.content,
		).toHaveLength(1000);
		expect(
			(events[1] as { sessionTurnDelta: { content: string } }).sessionTurnDelta
				.content,
		).toHaveLength(1000);
	});

	it("purges only the persisted session", () => {
		const buffer = new SessionLiveEventBuffer();
		buffer.append(event({ sessionCompleted: { session_id: "a" } }));
		buffer.append(event({ sessionCompleted: { session_id: "b" } }));
		buffer.purgeSession("a");
		expect(buffer.stats().map((entry) => entry.sessionId)).toEqual(["b"]);
	});

	it("selects only one session while retaining unscoped events", () => {
		const buffer = new SessionLiveEventBuffer();
		buffer.append(event({ providerCatalogUpdated: { provider_id: "codex" } }));
		buffer.append(event({ sessionCompleted: { session_id: "a" } }));
		buffer.append(event({ sessionCompleted: { session_id: "b" } }));

		expect(buffer.eventsForSession("a")).toHaveLength(2);
		expect(buffer.eventsForSession("a")[1]).toHaveProperty(
			"sessionCompleted.session_id",
			"a",
		);
		expect(buffer.eventsForSession("b")[1]).toHaveProperty(
			"sessionCompleted.session_id",
			"b",
		);
	});

	it("purges only through the confirmed turn and preserves the next turn", () => {
		const buffer = new SessionLiveEventBuffer();
		buffer.append(event({ sessionTurnStarted: { session_id: "a", turn_id: "turn-a" } }));
		buffer.append(event({ sessionTurnCompleted: { session_id: "a", turn_id: "turn-a" } }));
		buffer.append(event({ sessionTurnStarted: { session_id: "a", turn_id: "turn-b" } }));
		buffer.append(
			event({
				sessionTurnDelta: {
					session_id: "a",
					turn_id: "turn-b",
					content: "keep",
				},
			}),
		);
		buffer.purgeThroughTurn("a", "turn-a");
		expect(buffer.events()).toHaveLength(2);
		expect(buffer.events()[0]).toHaveProperty("sessionTurnStarted.turn_id", "turn-b");
	});

	it("does not combine distinct reasoning streams in the same turn", () => {
		const buffer = new SessionLiveEventBuffer();
		buffer.append(
			event({
				sessionTurnReasoningDelta: {
					session_id: "a",
					turn_id: "t",
					reasoning_id: "r1",
					content: "one",
				},
			}),
		);
		buffer.append(
			event({
				sessionTurnReasoningDelta: {
					session_id: "a",
					turn_id: "t",
					reasoning_id: "r2",
					content: "two",
				},
			}),
		);
		expect(buffer.events()).toHaveLength(2);
	});
});
