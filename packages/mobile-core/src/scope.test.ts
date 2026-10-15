import { describe, expect, it } from "vitest";
import { formatAgo, inScope, scopeTriage, shortModelName, type NeedsYouRow, type SessionSearchResult } from "./triage.ts";

function session(id: string, workspaceId: string | null): SessionSearchResult {
	return {
		sessionId: id,
		threadTitle: id,
		snippet: null,
		providerId: "claude",
		model: null,
		workspaceName: "ws",
		workspaceBranch: "main",
		workspaceId,
		projectId: null,
		updatedAt: "2026-10-15T12:00:00.000Z",
		archivedAt: null,
	};
}

function row(workspaceId: string, thread: SessionSearchResult | null = null): NeedsYouRow {
	return {
		entry: { workspaceId, reason: "permission", count: null, returnedAt: null, completedAt: null, sessionId: thread?.sessionId ?? null },
		thread,
		pending: null,
	};
}

describe("scopeTriage", () => {
	const groups = {
		needsYou: [row("a"), row("b")],
		running: [session("r1", "a"), session("r2", "c")],
		recent: [session("x", null), session("y", "b")],
	};

	it("keeps everything without a scope", () => {
		expect(scopeTriage(groups, null)).toBe(groups);
	});

	it("keeps only the project's workspaces", () => {
		const scoped = scopeTriage(groups, new Set(["a", "b"]));
		expect(scoped.needsYou.map((r) => r.entry.workspaceId)).toEqual(["a", "b"]);
		expect(scoped.running.map((s) => s.sessionId)).toEqual(["r1"]);
		expect(scoped.recent.map((s) => s.sessionId)).toEqual(["y"]);
	});

	it("drops a session with no workspace from a scoped view", () => {
		expect(inScope(null, new Set(["a"]))).toBe(false);
		expect(inScope(null, null)).toBe(true);
	});
});

describe("formatAgo", () => {
	const now = Date.parse("2026-10-15T12:00:00.000Z");
	const ago = (seconds: number) => new Date(now - seconds * 1000).toISOString();

	it("reads short", () => {
		expect(formatAgo(ago(20), now)).toBe("agora");
		expect(formatAgo(ago(12 * 60), now)).toBe("12 min");
		expect(formatAgo(ago(3 * 3600), now)).toBe("3 h");
		expect(formatAgo(ago(30 * 3600), now)).toBe("ontem");
		expect(formatAgo(ago(4 * 86400), now)).toBe("4 d");
		expect(formatAgo("not a date", now)).toBe("");
	});
});

describe("shortModelName", () => {
	it("shortens Claude ids and keeps the rest", () => {
		expect(shortModelName("claude-opus-4-1-20250805")).toBe("Opus 4.1");
		expect(shortModelName("claude-sonnet-5-5")).toBe("Sonnet 5.5");
		expect(shortModelName("claude-haiku-5")).toBe("Haiku 5");
		expect(shortModelName("gpt-5-codex")).toBe("gpt-5-codex");
		expect(shortModelName(null)).toBeNull();
		expect(shortModelName("  ")).toBeNull();
	});
});
