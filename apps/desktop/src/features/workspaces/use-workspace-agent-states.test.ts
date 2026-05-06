import { describe, expect, it } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import { deriveAgentStateFromSessions } from "./use-workspace-agent-states";

type SummaryOverrides = {
	session?: Partial<WorkspaceSessionSummary["session"]>;
	thread?: Partial<WorkspaceSessionSummary["thread"]>;
	projection?: Partial<WorkspaceSessionSummary["projection"]>;
	lastTurnPrompt?: WorkspaceSessionSummary["lastTurnPrompt"];
	lastTurnState?: WorkspaceSessionSummary["lastTurnState"];
};

function makeSummary(
	overrides: SummaryOverrides = {},
): WorkspaceSessionSummary {
	return {
		session: {
			id: "session-1",
			projectId: "project-1",
			workspaceId: "workspace-1",
			providerId: "codex",
			model: "gpt-5",
			state: "active",
			createdAt: "2026-05-06T12:00:00.000Z",
			updatedAt: "2026-05-06T12:00:00.000Z",
			...overrides.session,
		},
		thread: {
			id: "thread-1",
			project_id: "project-1",
			session_id: "session-1",
			title: "Thread 1",
			archived_at: null,
			...overrides.thread,
		},
		projection: {
			sessionId: "session-1",
			projectId: "project-1",
			workspaceId: "workspace-1",
			providerId: "codex",
			model: "gpt-5",
			state: "active",
			activeTurnId: null,
			turnCount: 0,
			checkpointCount: 0,
			createdAt: "2026-05-06T12:00:00.000Z",
			updatedAt: "2026-05-06T12:00:00.000Z",
			...overrides.projection,
		},
		lastTurnPrompt: overrides.lastTurnPrompt ?? null,
		lastTurnState: overrides.lastTurnState ?? null,
	};
}

describe("deriveAgentStateFromSessions", () => {
	it("returns active when any workspace session still has a running turn", () => {
		const completed = makeSummary({
			session: { id: "session-completed" },
			thread: { id: "thread-completed", session_id: "session-completed" },
			projection: { sessionId: "session-completed", turnCount: 1 },
			lastTurnState: "completed",
		});
		const running = makeSummary({
			session: { id: "session-running" },
			thread: { id: "thread-running", session_id: "session-running" },
			projection: { sessionId: "session-running", activeTurnId: "turn-1" },
			lastTurnState: "running",
		});

		expect(deriveAgentStateFromSessions([completed, running])).toBe("active");
	});

	it("returns completed after the turn finishes even if the session stays active", () => {
		const summary = makeSummary({
			projection: {
				state: "active",
				activeTurnId: null,
				turnCount: 1,
			},
			lastTurnState: "completed",
		});

		expect(deriveAgentStateFromSessions([summary])).toBe("completed");
	});

	it("returns aborted for the latest aborted turn", () => {
		const summary = makeSummary({
			projection: {
				state: "aborted",
				activeTurnId: null,
			},
			lastTurnState: "aborted",
		});

		expect(deriveAgentStateFromSessions([summary])).toBe("aborted");
	});

	it("returns null when the workspace has no executed turns yet", () => {
		const summary = makeSummary({});

		expect(deriveAgentStateFromSessions([summary])).toBeNull();
	});
});
