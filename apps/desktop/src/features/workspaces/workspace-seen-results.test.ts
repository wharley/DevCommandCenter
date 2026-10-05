import { describe, expect, it } from "vitest";
import { isWorkspaceResultUnread, withSeenWorkspaceResult } from "./workspace-seen-results";

describe("workspace seen results", () => {
	const baseline = "2026-10-05T12:00:00.000Z";

	it("treats results finished before the baseline as already seen", () => {
		expect(isWorkspaceResultUnread("2026-10-05T11:59:00.000Z", undefined, baseline)).toBe(false);
		expect(isWorkspaceResultUnread("2026-10-05T12:01:00.000Z", undefined, baseline)).toBe(true);
	});

	it("is unread until the person has seen a result at or after it", () => {
		const completedAt = "2026-10-05T13:00:00.000Z";
		expect(isWorkspaceResultUnread(completedAt, "2026-10-05T12:30:00.000Z", baseline)).toBe(true);
		expect(isWorkspaceResultUnread(completedAt, completedAt, baseline)).toBe(false);
	});

	it("compares instants, not strings, across timestamp formats", () => {
		expect(isWorkspaceResultUnread("2026-10-05 13:00:00Z", "2026-10-05T13:00:00.000Z", baseline)).toBe(
			false,
		);
	});

	it("stays quiet without a baseline or a timestamp", () => {
		expect(isWorkspaceResultUnread("2026-10-05T13:00:00.000Z", undefined, null)).toBe(false);
		expect(isWorkspaceResultUnread(null, undefined, baseline)).toBe(false);
	});

	it("keeps only the newest entries", () => {
		const seen = withSeenWorkspaceResult(
			{ a: "2026-10-05T10:00:00.000Z", b: "2026-10-05T11:00:00.000Z" },
			"c",
			"2026-10-05T12:00:00.000Z",
			2,
		);
		expect(Object.keys(seen).sort()).toEqual(["b", "c"]);
	});
});
