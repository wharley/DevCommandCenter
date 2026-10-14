import { describe, expect, it } from "vitest";
import { isDelegationStreamEvent, parseTaskProposals, proposalMeta, proposalsForSession } from "./taskProposals.ts";

const wire = {
	proposals: [
		{ id: "t-1", parentSessionId: "s1", workspaceId: "w1", title: "Fase 2: push", brief: "Implement push.", provider: "Codex", model: null, edits: true, createdAt: "2026-10-14T12:00:00Z" },
		{ id: "t-2", parentSessionId: "s2", title: "Ler o relay", brief: "Read it.", provider: "Cursor", model: "gpt-5.6", edits: false },
		{ id: "", parentSessionId: "s1" },
		{ id: "t-3" },
	],
};

describe("task proposals", () => {
	it("reads the list and drops malformed entries", () => {
		const proposals = parseTaskProposals(wire);
		expect(proposals.map((p) => p.id)).toEqual(["t-1", "t-2"]);
		expect(proposals[0]).toMatchObject({ title: "Fase 2: push", edits: true, model: null });
		expect(parseTaskProposals(null)).toEqual([]);
		expect(parseTaskProposals({ proposals: "x" })).toEqual([]);
	});

	it("picks a thread's proposals and says where each runs", () => {
		const proposals = parseTaskProposals(wire);
		expect(proposalsForSession(proposals, "s1").map((p) => p.id)).toEqual(["t-1"]);
		expect(proposalMeta(proposals[0]!)).toBe("Codex · worktree próprio");
		expect(proposalMeta(proposals[1]!)).toBe("gpt-5.6 · só leitura");
	});

	it("knows which stream events can change the list", () => {
		expect(isDelegationStreamEvent({ sessionDelegationRequested: { sessionId: "s1", delegationId: "t-1" } })).toBe(true);
		expect(isDelegationStreamEvent('{"sessionDelegationCancelled":{"sessionId":"s1"}}')).toBe(true);
		expect(isDelegationStreamEvent({ sessionTurnCompleted: { sessionId: "s1" } })).toBe(false);
		expect(isDelegationStreamEvent("not json")).toBe(false);
	});
});
