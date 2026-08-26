import { describe, expect, it } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import {
	deriveAgentActivityFromSessions,
	deriveAgentStateFromSessions,
	runningWorkspaceActivities,
} from "./use-workspace-agent-states";

type SummaryOverrides = {
	session?: Partial<WorkspaceSessionSummary["session"]>;
	thread?: Partial<WorkspaceSessionSummary["thread"]>;
	projection?: Partial<WorkspaceSessionSummary["projection"]>;
	lastTurnPrompt?: WorkspaceSessionSummary["lastTurnPrompt"];
	lastTurnState?: WorkspaceSessionSummary["lastTurnState"];
	lastTurnStartedAt?: WorkspaceSessionSummary["lastTurnStartedAt"];
	lastTurnCompletedAt?: WorkspaceSessionSummary["lastTurnCompletedAt"];
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
		lastTurnStartedAt: overrides.lastTurnStartedAt ?? null,
		lastTurnCompletedAt: overrides.lastTurnCompletedAt ?? null,
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
			lastTurnStartedAt: "2026-05-06T12:00:01.000Z",
			lastTurnCompletedAt: "2026-05-06T12:00:09.000Z",
		});

		expect(deriveAgentStateFromSessions([summary])).toBe("completed");
		expect(deriveAgentActivityFromSessions([summary])).toEqual({
			state: "completed",
			startedAt: "2026-05-06T12:00:01.000Z",
			completedAt: "2026-05-06T12:00:09.000Z",
		});
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

describe("runningWorkspaceActivities", () => {
	const workspace = (
		id: string,
		status: WorkspaceSummary["status"] = "ready",
	): WorkspaceSummary => ({
		id,
		name: `Task ${id}`,
		branch: "main",
		status,
	});

	it("keeps only active tasks and orders the newest execution first", () => {
		const result = runningWorkspaceActivities(
			[
				workspace("older"),
				workspace("completed-activity"),
				workspace("newer"),
				workspace("archived", "archived"),
			],
			{
				older: {
					state: "active",
					startedAt: "2026-08-26T12:00:00.000Z",
					completedAt: null,
				},
				"completed-activity": {
					state: "completed",
					startedAt: "2026-08-26T12:30:00.000Z",
					completedAt: "2026-08-26T12:31:00.000Z",
				},
				newer: {
					state: "active",
					startedAt: "2026-08-26T13:00:00.000Z",
					completedAt: null,
				},
				archived: {
					state: "active",
					startedAt: "2026-08-26T14:00:00.000Z",
					completedAt: null,
				},
			},
		);

		expect(result.map(({ workspace: entry }) => entry.id)).toEqual([
			"newer",
			"older",
		]);
	});

	it("uses workspace id as a stable fallback when start times are unavailable", () => {
		const result = runningWorkspaceActivities(
			[workspace("b"), workspace("a")],
			{
				a: { state: "active", startedAt: null, completedAt: null },
				b: { state: "active", startedAt: "invalid", completedAt: null },
			},
		);

		expect(result.map(({ workspace: entry }) => entry.id)).toEqual(["a", "b"]);
	});
});
