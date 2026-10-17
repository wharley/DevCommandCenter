import { describe, expect, it } from "vitest";
import { deltaPath, planDelta } from "./threadSync.ts";
import type { RawSessionEvent } from "./threadEvents.ts";

function event(sequence: number, type = "turn_delta"): RawSessionEvent {
	return {
		eventId: `e${sequence}`,
		sessionId: "s",
		sequence,
		occurredAt: "2026-10-08T00:00:00Z",
		kind: { type },
	};
}

describe("thread delta sync", () => {
	it("applies only events past the cursor and advances it", () => {
		const plan = planDelta(3, {
			events: [event(5), event(3), event(4)],
			lastSeq: 5,
			hasMore: false,
		});
		expect(plan).toEqual({
			kind: "apply",
			events: [event(4), event(5)],
			cursor: 5,
			more: false,
		});
	});

	it("keeps the cursor on an empty delta and reports more pages", () => {
		expect(planDelta(9, { events: [], lastSeq: 9, hasMore: false })).toMatchObject({
			kind: "apply",
			cursor: 9,
		});
		expect(
			planDelta(0, { events: [event(1)], lastSeq: 900, hasMore: true }),
		).toMatchObject({ kind: "apply", cursor: 1, more: true });
	});

	it("resets when the server's history is behind the cursor", () => {
		expect(planDelta(12, { events: [], lastSeq: 4, hasMore: false })).toEqual({
			kind: "reset",
			reason: "history-shrank",
		});
	});

	it("resets when the cursor fell inside merged history", () => {
		expect(
			planDelta(7, { events: [], lastSeq: 0, hasMore: false, reset: true }),
		).toEqual({ kind: "reset", reason: "history-compacted" });
	});

	it("resets when the delta carries a rewind", () => {
		expect(
			planDelta(2, {
				events: [event(3), event(4, "conversation_rewound")],
				lastSeq: 4,
				hasMore: false,
			}),
		).toEqual({ kind: "reset", reason: "rewind" });
	});

	it("asks for the delta after the cursor", () => {
		expect(deltaPath("a b", 7)).toBe(
			"/api/v1/sessions/a%20b/events?after=7&limit=500",
		);
	});
});
