import { afterEach, describe, expect, it } from "vitest";
import {
	bindPromptClock,
	resetStartupClockForTests,
	startPromptClock,
	turnClockOrigin,
	turnStartedAtWithOrigin,
} from "./startup-clock";

const SENT = Date.parse("2026-10-16T12:00:00.000Z");
const iso = (offsetMs: number) => new Date(SENT + offsetMs).toISOString();

describe("startup clock", () => {
	afterEach(() => resetStartupClockForTests());

	it("counts a turn from the send, not from when the backend started it", () => {
		startPromptClock(null, SENT);
		bindPromptClock({ turnId: "t1", sessionId: "s1", turnStartedAt: iso(18_000) });
		// The pending clock is spent; a later turn is not bound to it.
		bindPromptClock({ turnId: "t2", sessionId: "s1", turnStartedAt: iso(20_000) });
		expect(turnClockOrigin("t1")).toBe(SENT);
		expect(turnClockOrigin("t2")).toBeNull();
		expect(turnStartedAtWithOrigin(iso(18_000), SENT)).toBe(iso(0));
	});

	it("ignores turns that started before the send or in another session", () => {
		startPromptClock("s1", SENT);
		bindPromptClock({ turnId: "old", sessionId: "s1", turnStartedAt: iso(-60_000) });
		bindPromptClock({ turnId: "other", sessionId: "s2", turnStartedAt: iso(5_000) });
		bindPromptClock({ turnId: "late", sessionId: "s1", turnStartedAt: iso(10 * 60_000) });
		// None of them took the clock, so the right turn still can.
		expect([turnClockOrigin("old"), turnClockOrigin("other"), turnClockOrigin("late")]).toEqual([null, null, null]);
		bindPromptClock({ turnId: "mine", sessionId: "s1", turnStartedAt: iso(4_000) });
		expect(turnClockOrigin("mine")).toBe(SENT);
	});

	it("keeps the backend start when there is no earlier send", () => {
		expect(turnStartedAtWithOrigin(iso(5_000), null)).toBe(iso(5_000));
		expect(turnStartedAtWithOrigin(iso(5_000), SENT + 9_000)).toBe(iso(5_000));
		expect(turnStartedAtWithOrigin(undefined, SENT)).toBe(iso(0));
	});
});
