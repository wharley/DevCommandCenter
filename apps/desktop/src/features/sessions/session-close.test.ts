import { describe, expect, it } from "vitest";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import {
	isSessionArchived,
	isSessionEmpty,
	nextVisibleSessionIdAfterClose,
	shouldCreateReplacementSession,
	visibleSessions,
} from "./session-close";

function makeSession(
	id: string,
	options?: { archivedAt?: string | null; turnCount?: number },
): WorkspaceSessionSummary {
	return {
		session: {
			id,
			projectId: "project-1",
			workspaceId: "workspace-1",
			providerId: "codex",
			model: "gpt-5",
			state: "active",
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		},
		thread: {
			id: `thread-${id}`,
			project_id: "project-1",
			session_id: id,
			title: `Thread ${id}`,
			archived_at: options?.archivedAt ?? null,
		},
		projection: {
			sessionId: id,
			projectId: "project-1",
			workspaceId: "workspace-1",
			providerId: "codex",
			model: "gpt-5",
			state: "active",
			turnCount: options?.turnCount ?? 0,
			checkpointCount: 0,
			activeTurnId: null,
			createdAt: "2026-01-01T00:00:00Z",
			updatedAt: "2026-01-01T00:00:00Z",
		},
		lastTurnPrompt: null,
		lastTurnState: null,
		lastTurnStartedAt: null,
		lastTurnCompletedAt: null,
	};
}

describe("session-close", () => {
	it("filters archived sessions out of visible tabs", () => {
		const sessions = [
			makeSession("a"),
			makeSession("b", { archivedAt: "2026-01-01T00:00:00Z" }),
		];

		expect(visibleSessions(sessions).map((session) => session.session.id)).toEqual(["a"]);
		expect(isSessionArchived(sessions[1])).toBe(true);
	});

	it("detects empty sessions from turn count", () => {
		expect(isSessionEmpty(makeSession("a"))).toBe(true);
		expect(isSessionEmpty(makeSession("b", { turnCount: 2 }))).toBe(false);
	});

	it("picks another visible tab after closing one", () => {
		const sessions = [
			makeSession("a"),
			makeSession("b"),
			makeSession("c", { archivedAt: "2026-01-01T00:00:00Z" }),
		];

		expect(nextVisibleSessionIdAfterClose(sessions, "a")).toBe("b");
		expect(shouldCreateReplacementSession(sessions, "a")).toBe(false);
		expect(shouldCreateReplacementSession([makeSession("a")], "a")).toBe(true);
	});
});
