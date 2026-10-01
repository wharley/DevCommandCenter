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
 * One state for the agent across all its sessions, most urgent first. A
 * finished session only counts while its result is unread, so the sidebar
 * mascot never stays lit after the person has looked at it.
 */
export function aggregateAgentState(
	sessions: Array<{ state: AgentActivityState; unread: boolean }>,
): AgentActivityState {
	if (sessions.some((session) => session.state === "needsYou")) {
		return "needsYou";
	}
	if (sessions.some((session) => session.state === "working")) {
		return "working";
	}
	return sessions.some((session) => session.state === "done" && session.unread) ? "done" : "idle";
}
