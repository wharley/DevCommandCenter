import { describe, expect, it } from "vitest";
import type { SessionSearchResult } from "@dcc/contracts";
import type { WorkspaceSessionSummary } from "@dcc/contracts";
import type { WorkspaceSummary } from "./types";
import {
	filterPaletteSessionsByMetadata,
	groupPaletteSessions,
	MAX_RECENT_PALETTE_SESSIONS,
	matchesPaletteAction,
	recentPaletteSessions,
	rankPaletteWorkspaces,
	resolvePaletteSessionSearch,
} from "./workspace-command-palette.logic";

const workspace = (
	id: string,
	name: string,
	branch: string,
): WorkspaceSummary => ({ id, name, branch, status: "ready" });

const session = (sessionId: string, workspaceId: string): SessionSearchResult =>
	({ sessionId, workspaceId }) as SessionSearchResult;

const recentSession = (
	id: number,
	updatedAt: string,
): WorkspaceSessionSummary =>
	({
		session: {
			id: `session-${id}`,
			projectId: "project",
			workspaceId: "workspace",
			providerId: "codex",
			model: null,
			state: "completed",
			createdAt: updatedAt,
			updatedAt,
		},
		thread: {
			id: `thread-${id}`,
			project_id: "project",
			session_id: `session-${id}`,
			title: `Session ${id}`,
			archived_at: null,
		},
		projection: {
			sessionId: `session-${id}`,
			projectId: "project",
			workspaceId: "workspace",
			providerId: "codex",
			model: null,
			state: "completed",
			activeTurnId: null,
			turnCount: 1,
			checkpointCount: 0,
			createdAt: updatedAt,
			updatedAt,
		},
		lastTurnPrompt: null,
		lastTurnState: null,
		lastTurnStartedAt: null,
		lastTurnCompletedAt: updatedAt,
	} as WorkspaceSessionSummary);

describe("unified command palette logic", () => {
	it("ranks exact workspace names ahead of partial matches and hides inactive workspaces", () => {
		const results = rankPaletteWorkspaces(
			[
				workspace("one", "DCC API", "main"),
				workspace("two", "DCC", "feature/palette"),
				{ ...workspace("three", "DCC archive", "main"), status: "archived" },
			],
			"dcc",
			"one",
		);

		expect(results.map((item) => item.id)).toEqual(["two", "one"]);
	});

	it("matches actions against their label and search keywords", () => {
		expect(
			matchesPaletteAction(
				{ label: "Open Inspector changes", keywords: "git diff review" },
				"diff",
			),
		).toBe(true);
		expect(
			matchesPaletteAction(
				{ label: "Open settings", keywords: "preferences" },
				"terminal",
			),
		).toBe(false);
		expect(
			matchesPaletteAction(
				{ label: "Open Inspector changes", keywords: "git diff review" },
				"git review",
			),
		).toBe(true);
	});

	it("tokenizes workspace ranking across name and branch", () => {
		const results = rankPaletteWorkspaces(
			[
				workspace("one", "DCC", "feature/palette"),
				workspace("two", "DCC API", "main"),
			],
			"dcc palette",
			null,
		);

		expect(results.map((item) => item.id)).toEqual(["one"]);
	});

	it("enables session FTS only for an explicit, sufficiently long @ query", () => {
		expect(resolvePaletteSessionSearch("project")).toMatchObject({
			isExplicit: false,
			enabled: false,
		});
		expect(resolvePaletteSessionSearch("@")).toMatchObject({
			isExplicit: true,
			enabled: false,
		});
		expect(resolvePaletteSessionSearch("@ a")).toMatchObject({
			isExplicit: true,
			enabled: false,
		});
		expect(resolvePaletteSessionSearch("@ fix")).toEqual({
			query: "fix",
			isExplicit: true,
			enabled: true,
		});
	});

	it("keeps current-workspace sessions in their own group", () => {
		const results = groupPaletteSessions(
			[
				session("a", "one"),
				session("b", "two"),
			],
			"one",
		);

		expect(results.currentWorkspace.map((item) => item.sessionId)).toEqual(["a"]);
		expect(results.otherWorkspaces.map((item) => item.sessionId)).toEqual(["b"]);
	});

	it("filters recent sessions by metadata without inspecting prompt content", () => {
		const results = filterPaletteSessionsByMetadata(
			[
				{
					...session("a", "one"),
					threadTitle: "Fix checkout",
					workspaceName: "Storefront",
					workspaceBranch: "feature/cart",
					snippet: "unrelated secret prompt",
				},
			],
			"store cart",
		);

		expect(results.map((item) => item.sessionId)).toEqual(["a"]);
		expect(filterPaletteSessionsByMetadata(results, "secret")).toEqual([]);
	});

	it("limits recent metadata to the 40 most recently updated sessions", () => {
		const sessions = Array.from({ length: 42 }, (_, index) =>
			recentSession(index, new Date(Date.UTC(2026, 0, index + 1)).toISOString()),
		);
		const results = recentPaletteSessions(sessions, {
			name: "DCC",
			branch: "main",
			rootPath: "/dcc",
			worktreePath: null,
		});

		expect(results).toHaveLength(MAX_RECENT_PALETTE_SESSIONS);
		expect(results[0]?.sessionId).toBe("session-41");
		expect(results.at(-1)?.sessionId).toBe("session-2");
	});
});
