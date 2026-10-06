import { describe, expect, it } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import {
	deriveAgentActivityFromSessions,
	deriveAgentStateFromSessions,
	deriveProviderIdFromSessions,
	attentionWorkspaceItems,
	lastInteractionAtFromSessions,
	runningWorkspaceActivities,
	type WorkspaceAgentActivity,
} from "./use-workspace-agent-states";

type SummaryOverrides = {
	session?: Partial<WorkspaceSessionSummary["session"]>;
	thread?: Partial<WorkspaceSessionSummary["thread"]>;
	projection?: Partial<WorkspaceSessionSummary["projection"]>;
	lastTurnPrompt?: WorkspaceSessionSummary["lastTurnPrompt"];
	lastTurnState?: WorkspaceSessionSummary["lastTurnState"];
	lastTurnStartedAt?: WorkspaceSessionSummary["lastTurnStartedAt"];
	lastTurnCompletedAt?: WorkspaceSessionSummary["lastTurnCompletedAt"];
	lastTurnAwaitingUser?: WorkspaceSessionSummary["lastTurnAwaitingUser"];
	lastTurnAwaitingSince?: WorkspaceSessionSummary["lastTurnAwaitingSince"];
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
		lastTurnAwaitingUser: overrides.lastTurnAwaitingUser ?? null,
		lastTurnAwaitingSince: overrides.lastTurnAwaitingSince ?? null,
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

	it("returns waiting, with the reason, while the running turn is blocked on the user", () => {
		const running = makeSummary({
			session: { id: "session-running" },
			projection: { sessionId: "session-running", activeTurnId: "turn-2" },
			lastTurnState: "running",
			lastTurnStartedAt: "2026-09-30T10:00:00.000Z",
		});
		const blocked = makeSummary({
			session: { id: "session-blocked" },
			projection: { sessionId: "session-blocked", activeTurnId: "turn-1" },
			lastTurnState: "running",
			lastTurnStartedAt: "2026-09-30T09:00:00.000Z",
			lastTurnAwaitingUser: "permission",
			lastTurnAwaitingSince: "2026-09-30T09:40:00.000Z",
		});

		// Another session still working must not hide the one that needs the user.
		expect(deriveAgentActivityFromSessions([running, blocked])).toEqual({
			state: "waiting",
			startedAt: "2026-09-30T09:00:00.000Z",
			completedAt: null,
			waitingFor: "permission",
			waitingSince: "2026-09-30T09:40:00.000Z",
		});
		expect(
			deriveAgentActivityFromSessions([
				makeSummary({
					projection: { activeTurnId: "turn-1" },
					lastTurnState: "running",
					lastTurnAwaitingUser: "input",
				}),
			])?.waitingFor,
		).toBe("input");
	});

	it("ignores a stale awaiting flag once the turn is no longer running", () => {
		expect(
			deriveAgentStateFromSessions([
				makeSummary({
					projection: { turnCount: 1 },
					lastTurnState: "completed",
					lastTurnAwaitingUser: "permission",
				}),
			]),
		).toBe("completed");
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

describe("deriveProviderIdFromSessions", () => {
	it("prefers the provider of a running session", () => {
		const latest = makeSummary({
			session: { providerId: "gemini" },
		});
		const running = makeSummary({
			session: { id: "session-running", providerId: "claude_code" },
			thread: { id: "thread-running", session_id: "session-running" },
			projection: { sessionId: "session-running", activeTurnId: "turn-1" },
			lastTurnState: "running",
		});

		expect(deriveProviderIdFromSessions([latest, running])).toBe("claude_code");
	});

	it("falls back to the latest session provider", () => {
		expect(deriveProviderIdFromSessions([makeSummary({ session: { providerId: "codex" } })])).toBe(
			"codex",
		);
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

	it("keeps tasks waiting on the user in the list and puts them first", () => {
		const result = runningWorkspaceActivities(
			[workspace("working"), workspace("blocked")],
			{
				working: {
					state: "active",
					startedAt: "2026-09-30T11:00:00.000Z",
					completedAt: null,
				},
				blocked: {
					state: "waiting",
					startedAt: "2026-09-30T10:00:00.000Z",
					completedAt: null,
					waitingFor: "permission",
				},
			},
		);

		expect(result.map(({ workspace: entry }) => entry.id)).toEqual([
			"blocked",
			"working",
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

describe("lastInteractionAtFromSessions", () => {
	it("uses the newest turn start across sessions and ignores completion", () => {
		expect(
			lastInteractionAtFromSessions([
				makeSummary({
					lastTurnStartedAt: "2026-09-30T10:00:00.000Z",
					lastTurnCompletedAt: "2026-09-30T18:00:00.000Z",
				}),
				makeSummary({
					session: { id: "session-2" },
					lastTurnStartedAt: "2026-09-30T12:00:00.000Z",
				}),
				makeSummary({ session: { id: "session-3" }, lastTurnStartedAt: null }),
			]),
		).toBe("2026-09-30T12:00:00.000Z");
	});

	it("returns null when no turn has started", () => {
		expect(lastInteractionAtFromSessions([makeSummary()])).toBeNull();
	});
});

describe("attentionWorkspaceItems", () => {
	const workspace = (
		id: string,
		status: WorkspaceSummary["status"] = "ready",
	): WorkspaceSummary => ({ id, name: `Task ${id}`, branch: "main", status });
	const finished = (
		state: "completed" | "aborted",
		completedAt: string,
	): WorkspaceAgentActivity => ({ state, startedAt: null, completedAt });

	it("lists what needs the person, newest return first, and leaves running agents out", () => {
		const result = attentionWorkspaceItems(
			[
				workspace("running"),
				workspace("done-unread"),
				workspace("done-read"),
				workspace("question"),
				workspace("approval"),
				workspace("setup", "setup_pending"),
				workspace("interrupted"),
				workspace("archived-waiting", "archived"),
			],
			{
				running: { state: "active", startedAt: "2026-10-05T10:00:00.000Z", completedAt: null },
				"done-unread": finished("completed", "2026-10-05T11:00:00.000Z"),
				"done-read": finished("completed", "2026-10-05T11:30:00.000Z"),
				question: {
					state: "waiting",
					waitingFor: "input",
					startedAt: "2026-10-05T09:00:00.000Z",
					completedAt: null,
				},
				// A long turn that asked for permission just now came back last.
				approval: {
					state: "waiting",
					waitingFor: "permission",
					startedAt: "2026-10-05T08:00:00.000Z",
					waitingSince: "2026-10-05T11:45:00.000Z",
					completedAt: null,
				},
				interrupted: finished("aborted", "2026-10-05T10:30:00.000Z"),
				"archived-waiting": {
					state: "waiting",
					waitingFor: "input",
					startedAt: null,
					completedAt: null,
				},
			},
			(workspaceId) => workspaceId !== "done-read",
		);

		expect(result.map((item) => [item.workspace.id, item.reason])).toEqual([
			["approval", "permission"],
			["done-unread", "completed"],
			["interrupted", "aborted"],
			["question", "input"],
			["setup", "setup"],
		]);
	});

	it("does not flag setup while an agent is already running there", () => {
		expect(
			attentionWorkspaceItems(
				[workspace("setup", "setup_pending")],
				{ setup: { state: "active", startedAt: null, completedAt: null } },
				() => true,
			),
		).toEqual([]);
	});
});

describe("attentionWorkspaceItems with backend blockers", () => {
	const workspace = (id: string, updatedAt?: string): WorkspaceSummary => ({
		id,
		name: id,
		branch: "main",
		status: "ready",
		updatedAt,
	});

	it("surfaces git and review blockers by when they appeared, but not over a running agent", () => {
		const result = attentionWorkspaceItems(
			[
				workspace("checks"),
				workspace("conflict", "2026-10-06T09:00:00.000Z"),
				workspace("busy"),
				workspace("review"),
			],
			{ busy: { state: "active", startedAt: null, completedAt: null } },
			() => false,
			new Map([
				[
					"checks",
					[
						{
							workspaceId: "checks",
							kind: "checks_failing",
							count: 12,
							since: "2026-10-06T10:00:00.000Z",
						},
					],
				],
				// Already there at startup: falls back to the task's last update.
				["conflict", [{ workspaceId: "conflict", kind: "conflicts", count: null, since: null }]],
				["busy", [{ workspaceId: "busy", kind: "conflicts", count: null, since: null }]],
				[
					"review",
					[
						{
							workspaceId: "review",
							kind: "delegated_edits_review",
							count: 2,
							since: "2026-10-06T08:00:00.000Z",
						},
						{
							workspaceId: "review",
							kind: "pr_conflicts",
							count: 7,
							since: "2026-10-06T11:00:00.000Z",
						},
					],
				],
			]),
		);
		expect(result.map((item) => [item.workspace.id, item.reason, item.count])).toEqual([
			["review", "prConflicts", 7],
			["checks", "checksFailing", 12],
			["conflict", "conflicts", null],
		]);
	});

	it("keeps an agent waiting on the person ahead of a git blocker", () => {
		const result = attentionWorkspaceItems(
			[workspace("w")],
			{ w: { state: "waiting", waitingFor: "permission", startedAt: null, completedAt: null } },
			() => false,
			new Map([["w", [{ workspaceId: "w", kind: "conflicts", count: null, since: null }]]]),
		);
		expect(result.map((item) => item.reason)).toEqual(["permission"]);
	});
});

describe("attentionWorkspaceItems with snoozes", () => {
	const now = Date.parse("2026-10-07T12:00:00Z");
	const task = (id: string, snoozedUntil: string | null): WorkspaceSummary => ({
		id,
		name: id,
		branch: "main",
		status: "ready",
		snoozedUntil,
	});

	it("brings a woken task back and hides one still snoozed", () => {
		const result = attentionWorkspaceItems(
			[task("woken", "2026-10-07T11:00:00Z"), task("later", "2026-10-07T13:00:00Z")],
			{
				later: { state: "waiting", waitingFor: "input", startedAt: null, completedAt: null },
			},
			() => false,
			new Map([["later", [{ workspaceId: "later", kind: "conflicts", count: null, since: null }]]]),
			now,
		);
		expect(result.map((item) => [item.workspace.id, item.reason])).toEqual([["woken", "woke"]]);
	});
});
