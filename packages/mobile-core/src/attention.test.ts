import { describe, expect, it } from "vitest";
import { attentionLabel, isResultUnread, needsYouEntries, parseAttention, withSeenResult } from "./attention.ts";

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
