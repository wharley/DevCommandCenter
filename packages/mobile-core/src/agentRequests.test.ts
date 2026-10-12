import { describe, expect, it } from "vitest";
import {
	agentRequestsForSession,
	allowBlocker,
	conflictOutcome,
	outcomeText,
	parseAgentRequests,
	secondsLeft,
} from "./agentRequests.ts";

const now = 1_000_000;
const wire = {
	requests: [
		{ requestId: "bo-1", sessionId: "s1", workspaceId: "w1", providerId: "claude", reason: "docs", expiresAtMs: now + 30_000, kind: "browserOpen", url: "https://docs.example.com" },
		{ requestId: "cu-1", sessionId: "s2", workspaceId: "w2", providerId: "codex", reason: "abrir o Figma", expiresAtMs: now + 10_500, kind: "computerControl", apps: [{ bundleId: "com.figma.Desktop", name: "Figma" }, { bundleId: "" }], blocker: null },
		{ requestId: "x", sessionId: "s1", kind: "somethingNew" },
		{ requestId: "bo-2", sessionId: "s1", expiresAtMs: now - 1, kind: "browserOpen", url: "https://old.example" },
	],
};

describe("agent requests", () => {
	it("parses both kinds as the desktop serves them and drops unknown ones", () => {
		const requests = parseAgentRequests(wire);
		expect(requests.map((r) => r.requestId)).toEqual(["bo-1", "cu-1", "bo-2"]);
		const computer = requests[1]!;
		expect(computer.kind === "computerControl" && computer.apps).toEqual([{ bundleId: "com.figma.Desktop", name: "Figma" }]);
		expect(parseAgentRequests(null)).toEqual([]);
	});

	it("keeps only open requests of the thread", () => {
		const requests = parseAgentRequests(wire);
		expect(agentRequestsForSession(requests, "s1", now).map((r) => r.requestId)).toEqual(["bo-1"]);
		expect(secondsLeft(requests[1]!, now)).toBe(11);
	});

	it("says why Permitir is off", () => {
		const [browser, computer, expired] = parseAgentRequests(wire);
		expect(allowBlocker(browser!, [], now)).toBeNull();
		expect(allowBlocker(computer!, [], now)).toBe("Marque pelo menos um app.");
		expect(allowBlocker(computer!, ["com.figma.Desktop"], now)).toBeNull();
		expect(allowBlocker({ ...computer!, blocker: "Conceda Acessibilidade" } as never, ["a"], now)).toBe("Conceda Acessibilidade");
		expect(allowBlocker(expired!, [], now)).toBe("O pedido expirou.");
	});

	it("reads who answered first from a 409", () => {
		const outcome = conflictOutcome({ ok: false, error: { code: "already_resolved", outcome: { requestId: "bo-1", decision: "denied", by: "desktop", at: "t" } } });
		expect(outcomeText(outcome)).toBe("Esse pedido já foi negado no computador.");
		expect(outcomeText(conflictOutcome({ error: { outcome: { kind: "permission" } } }))).toBe(
			"Esse pedido já tinha sido respondido em outro lugar.",
		);
	});
});
