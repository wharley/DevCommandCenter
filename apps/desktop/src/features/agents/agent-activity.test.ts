import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { agentSessionState, aggregateAgentState } from "./agent-activity";

function summary(
	lastTurnState: string | null,
	lastTurnAwaitingUser: string | null = null,
): WorkspaceSessionSummary {
	return {
		projection: { state: "active", activeTurnId: null },
		lastTurnState,
		lastTurnAwaitingUser,
	} as unknown as WorkspaceSessionSummary;
}

describe("agent activity", () => {
	it("derives a session state from its summary", () => {
		expect(agentSessionState(undefined)).toBe("idle");
		expect(agentSessionState(summary(null))).toBe("idle");
		expect(agentSessionState(summary("running"))).toBe("working");
		expect(agentSessionState(summary("running", "permission"))).toBe("needsYou");
		expect(agentSessionState(summary("completed"))).toBe("done");
		// A finished turn cannot be waiting on the person.
		expect(agentSessionState(summary("completed", "input"))).toBe("done");
		expect(agentSessionState(summary("aborted"))).toBe("idle");
	});

	it("aggregates to the most urgent state and lights up only for unread results", () => {
		const read = { state: "done", unread: false } as const;
		const unread = { state: "done", unread: true } as const;
		const working = { state: "working", unread: false } as const;
		const needsYou = { state: "needsYou", unread: false } as const;
		expect(aggregateAgentState([])).toBe("idle");
		expect(aggregateAgentState([read])).toBe("idle");
		expect(aggregateAgentState([read, unread])).toBe("done");
		expect(aggregateAgentState([unread, working])).toBe("working");
		expect(aggregateAgentState([working, needsYou])).toBe("needsYou");
	});
});
