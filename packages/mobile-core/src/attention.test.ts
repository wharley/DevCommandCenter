import { describe, expect, it } from "vitest";
import { attentionLabel, isResultUnread, needsYouEntries, parseAttention, parseHomeSnapshot, withSeenResult } from "./attention.ts";

const wire = {
	items: [
		{ workspaceId: "w1", reason: "checksFailing", count: 42, returnedAt: "2026-10-12T11:00:00Z", completedAt: null, sessionId: "s1" },
		{ workspaceId: "w2", reason: "completed", count: null, returnedAt: "2026-10-12T10:00:00Z", completedAt: "2026-10-12T10:00:00Z", sessionId: "s2" },
		{ workspaceId: "w3", reason: "delegatedReview", count: 2, returnedAt: null, completedAt: null, sessionId: null },
		{ workspaceId: "w4", reason: "whatever" },
	],
};

describe("attention", () => {
	it("parses the desktop's list in its order and drops unknown reasons", () => {
		expect(parseAttention(wire).map((e) => e.workspaceId)).toEqual(["w1", "w2", "w3"]);
	});

	it("labels each reason with the desktop's words", () => {
		const [checks, done, review] = parseAttention(wire);
		expect(attentionLabel(checks!)).toBe("Checks falhando no PR #42");
		expect(attentionLabel(done!)).toBe("Resultado novo");
		expect(attentionLabel(review!)).toBe("2 mudanças delegadas para revisar");
		expect(attentionLabel({ reason: "delegatedReview", count: 1 })).toBe("1 mudança delegada para revisar");
		expect(attentionLabel({ reason: "permission", count: null })).toBe("Aguardando aprovação");
	});

	it("keeps a finished result only until it is opened on this phone", () => {
		const entries = parseAttention(wire);
		const baseline = "2026-10-12T09:00:00Z";
		expect(needsYouEntries(entries, {}, baseline).map((e) => e.workspaceId)).toEqual(["w1", "w2", "w3"]);
		const seen = withSeenResult({}, "w2", "2026-10-12T10:00:00Z");
		expect(needsYouEntries(entries, seen, baseline).map((e) => e.workspaceId)).toEqual(["w1", "w3"]);
		// Results from before the phone started tracking are not news; no baseline, nothing unread.
		expect(isResultUnread("2026-10-12T08:00:00Z", undefined, baseline)).toBe(false);
		expect(isResultUnread("2026-10-12T10:00:00Z", undefined, null)).toBe(false);
	});
});

describe("parseHomeSnapshot", () => {
	it("reads what runs, the open requests and the snapshot id", () => {
		const snapshot = parseHomeSnapshot({
			...wire,
			snapshotId: "abc",
			running: [{ workspaceId: "w9", sessionId: "s9", sessionIds: ["s9", "s8"], startedAt: "2026-10-16T13:04:06Z" }, { bad: true }],
			requests: [{ eventId: "e", sessionId: "s1", sequence: 3, occurredAt: "x", kind: { type: "turn_permission_requested" } }, null],
		});
		expect(snapshot.unchanged).toBe(false);
		if (snapshot.unchanged) return;
		expect(snapshot.snapshotId).toBe("abc");
		expect(snapshot.running).toEqual([{ workspaceId: "w9", sessionId: "s9", sessionIds: ["s9", "s8"], startedAt: "2026-10-16T13:04:06Z" }]);
		expect(snapshot.requests).toHaveLength(1);
		expect(snapshot.entries.map((e) => e.workspaceId)).toEqual(["w1", "w2", "w3"]);
	});

	it("takes unchanged only as the answer to the id it asked about", () => {
		expect(parseHomeSnapshot({ unchanged: true, snapshotId: "abc" }, "abc")).toEqual({ unchanged: true, snapshotId: "abc" });
		expect(parseHomeSnapshot({ unchanged: true, snapshotId: "abc" }, "old").unchanged).toBe(false);
	});

	it("leaves running and requests null on desktops that predate them", () => {
		const snapshot = parseHomeSnapshot(wire);
		expect(snapshot).toMatchObject({ unchanged: false, snapshotId: null, running: null, requests: null });
	});
});
