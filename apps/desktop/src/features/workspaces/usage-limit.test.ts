import { describe, expect, it } from "vitest";
import type { SessionUsageLimit } from "@dcc/contracts";
import { formatLimitReset, usageLimitStatus } from "./usage-limit";

const limit = (overrides: Partial<SessionUsageLimit> = {}): SessionUsageLimit => ({
	turnId: "t1",
	hitAt: "2026-10-12T01:30:00Z",
	resetsAt: "2026-10-12T03:40:00Z",
	autoResume: false,
	autoResumesUsed: 0,
	...overrides,
});

const at = (value: string) => Date.parse(value);

describe("usageLimitStatus", () => {
	it("waits until the reset, then needs the person when nothing resumes it", () => {
		expect(usageLimitStatus(limit(), at("2026-10-12T02:00:00Z")).phase).toBe("waiting");
		expect(usageLimitStatus(limit(), at("2026-10-12T03:41:00Z"))).toEqual({
			phase: "paused",
			pauseReason: "reset_passed",
			returnedAt: "2026-10-12T03:40:00Z",
		});
	});

	it("keeps waiting past the reset when it resumes by itself", () => {
		expect(
			usageLimitStatus(limit({ autoResume: true }), at("2026-10-12T09:00:00Z")).phase,
		).toBe("waiting");
	});

	it("never guesses an unknown reset and stops after three automatic resumes", () => {
		expect(usageLimitStatus(limit({ resetsAt: null, autoResume: true })).pauseReason).toBe(
			"unknown_reset",
		);
		expect(
			usageLimitStatus(limit({ autoResume: true, autoResumesUsed: 3 }), at("2026-10-12T02:00:00Z"))
				.pauseReason,
		).toBe("attempts_exhausted");
	});
});

describe("formatLimitReset", () => {
	it("shows only the time today and the day otherwise", () => {
		const today = new Date(2026, 9, 12, 1, 0).getTime();
		expect(formatLimitReset(new Date(2026, 9, 12, 3, 40).toISOString(), today)).toBe("03:40");
		expect(formatLimitReset(new Date(2026, 9, 13, 3, 40).toISOString(), today)).toBe(
			"13/10 03:40",
		);
		expect(formatLimitReset(null, today)).toBeNull();
	});
});
