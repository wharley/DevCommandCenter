import { describe, expect, it } from "vitest";
import { isResultUnread, withSeenResult } from "./agent-seen-results";

describe("seen agent results", () => {
	it("is unread until a turn finished at or after it was seen", () => {
		const done = "2026-10-01T10:00:00.000Z";
		expect(isResultUnread(done, undefined)).toBe(true);
		expect(isResultUnread(done, "2026-10-01T09:00:00.000Z")).toBe(true);
		expect(isResultUnread(done, done)).toBe(false);
		// Nothing finished yet: nothing to read.
		expect(isResultUnread(null, undefined)).toBe(false);
	});

	it("keeps only the newest markers", () => {
		const seen = withSeenResult(
			{ a: "2026-10-01T08:00:00Z", b: "2026-10-01T09:00:00Z" },
			"c",
			"2026-10-01T10:00:00Z",
			2,
		);
		expect(Object.keys(seen).sort()).toEqual(["b", "c"]);
	});
});
