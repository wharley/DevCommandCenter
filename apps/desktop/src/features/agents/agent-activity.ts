import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type { AgentActivityState } from "./agent-avatar";

/** What one agent session is doing, from the same summary the rail uses. */
export function agentSessionState(
	summary: WorkspaceSessionSummary | undefined,
): AgentActivityState {
	if (!summary) {
		return "idle";
	}
	const running =
		summary.lastTurnState === "running" ||
		(summary.projection.state === "active" && Boolean(summary.projection.activeTurnId));
	if (running) {
		return summary.lastTurnAwaitingUser === "permission" ||
			summary.lastTurnAwaitingUser === "input"
			? "needsYou"
			: "working";
	}
	return summary.lastTurnState === "completed" ? "done" : "idle";
}

/**
 * One state for the agent across all its sessions. A finished review does not
 * count here: the sidebar mascot only signals work in progress or a blocked
 * session, so it never stays lit after the person has moved on.
 */
export function aggregateAgentState(states: AgentActivityState[]): AgentActivityState {
	if (states.includes("needsYou")) {
		return "needsYou";
	}
	return states.includes("working") ? "working" : "idle";
}
