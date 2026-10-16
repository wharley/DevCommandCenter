import { describe, expect, it } from "vitest";
import type { RawSessionEvent } from "./threadEvents.ts";
import {
	classifySession,
	liveRunning,
	liveSessions,
	runScan,
	scanFromSnapshot,
	triageGroups,
	withLiveOverlay,
	type Comb,
	type SessionSearchResult,
} from "./triage.ts";

function session(id: string, extra: Partial<SessionSearchResult> = {}): SessionSearchResult {
	return {
		sessionId: id,
		threadTitle: id,
		snippet: null,
		providerId: "claude",
		model: null,
		workspaceName: "ws",
		workspaceBranch: "main",
		workspaceId: "w1",
		projectId: "p",
		updatedAt: new Date().toISOString(),
		archivedAt: null,
		...extra,
	};
}

let seq = 0;
function ev(type: string, extra: Record<string, unknown> = {}): RawSessionEvent {
	seq += 1;
	return {
		eventId: `e${seq}`,
		sessionId: "s",
		sequence: seq,
		occurredAt: `2026-10-09T00:00:${String(seq).padStart(2, "0")}Z`,
		kind: { type, ...extra },
	};
}

describe("classifySession", () => {
	it("marks a live turn with an open permission", () => {
		const result = classifySession(session("a"), [
			ev("turn_started", { turnId: "t" }),
			ev("turn_permission_requested", { turnId: "t", requestId: "r1", question: "rm?" }),
			ev("turn_permission_requested", { turnId: "t", requestId: "r2" }),
			ev("turn_permission_requested", { turnId: "t", requestId: "r3", description: "Allow network?", command: "yarn test" }),
			ev("turn_permission_resolved", { turnId: "t", requestId: "r2" }),
		]);
		expect(result.status).toBe("running");
		expect(result.needsYou.map((p) => p.requestId)).toEqual(["r1", "r3"]);
		// Durable events carry title/description/command, not `question`.
		expect(result.needsYou[1]?.question).toBe("Allow network?\nyarn test");
		expect(result.needsYou[0]?.choices.map((c) => c.id)).toEqual(["allow", "deny"]);
	});

	it("reports agent questions without inline choices", () => {
		const result = classifySession(session("a"), [
			ev("turn_started", { turnId: "t" }),
			ev("turn_user_input_requested", {
				turnId: "t",
				requestId: "q1",
				questions: [{ id: "x", header: "", question: "Qual?", options: [] }],
			}),
			ev("turn_completed", { turnId: "t" }),
		]);
		expect(result.status).toBe("completed");
		expect(result.needsYou).toMatchObject([{ requestId: "q1", question: "Qual?", choices: [] }]);
	});
});

describe("triage", () => {
	it("drops archived sessions and archived workspaces", () => {
		const combs: Comb[] = [
			{ id: "w1", name: null, branch: null, projectId: null, projectName: null, worktreePath: null, status: "ready", lastOpenedAt: null },
			{ id: "w2", name: null, branch: null, projectId: null, projectName: null, worktreePath: null, status: "archived", lastOpenedAt: null },
		];
		const kept = liveSessions(
			[session("a"), session("b", { workspaceId: "w2" }), session("c", { archivedAt: "x" })],
			combs,
		);
		expect(kept.map((s) => s.sessionId)).toEqual(["a"]);
	});

	it("splits needs-you, running and recent without repeats", async () => {
		const histories: Record<string, RawSessionEvent[]> = {
			a: [ev("turn_started", { turnId: "t" }), ev("turn_permission_requested", { turnId: "t", requestId: "r" })],
			b: [ev("turn_started", { turnId: "t" })],
			c: [ev("turn_started", { turnId: "t" }), ev("turn_completed", { turnId: "t" })],
		};
		const fetchJson = async <T,>(path: string): Promise<T> => {
			if (path.startsWith("/api/v1/sessions/search")) {
				return [session("a"), session("b"), session("c")] as T;
			}
			if (path === "/api/v1/combs") return [] as T;
			const id = /sessions\/([^/]+)\/events/.exec(path)?.[1] ?? "";
			return (histories[id] ?? []) as T;
		};
		const { scan } = await runScan(fetchJson);
		const groups = triageGroups(scan);
		expect(groups.needsYou.map((p) => p.thread.sessionId)).toEqual(["a"]);
		expect(groups.running.map((s) => s.sessionId)).toEqual(["b"]);
		expect(groups.recent.map((s) => s.sessionId)).toEqual(["c"]);
	});
});

describe("triage with the desktop's attention list", () => {
	it("keeps the desktop's order, attaches inline answers and takes those tasks off the other groups", async () => {
		const { triageWithAttention } = await import("./triage.ts");
		const a = session("a", { workspaceId: "wa" });
		const b = session("b", { workspaceId: "wb" });
		const c = session("c", { workspaceId: "wc" });
		const d = session("d", { workspaceId: "wd" });
		const scan = {
			sessions: [a, b, c, d],
			statuses: new Map([
				["a", "running" as const],
				["c", "running" as const],
				["d", "running" as const],
			]),
			needsYou: [{ thread: a, requestId: "p1", question: "rodar yarn test?", choices: [{ id: "allow", label: "Permitir" }], at: "x" }],
		};
		const entries = [
			{ workspaceId: "wb", reason: "checksFailing" as const, count: 7, returnedAt: "2026-10-12T11:00:00Z", completedAt: null, sessionId: "b" },
			{ workspaceId: "wa", reason: "permission" as const, count: null, returnedAt: "2026-10-12T10:00:00Z", completedAt: null, sessionId: "a" },
		];
		const groups = triageWithAttention(scan, entries, new Set(["d"]));
		expect(groups.needsYou.map((row) => [row.entry.workspaceId, row.pending?.requestId ?? null])).toEqual([
			["wb", null],
			["wa", "p1"],
		]);
		expect(groups.running.map((s) => s.sessionId)).toEqual(["c"]);
		expect(groups.recent.map((s) => s.sessionId)).toEqual([]);
	});
});

describe("scan from the desktop's snapshot", () => {
	it("runs what the desktop says runs and answers its open requests inline", () => {
		const a = session("a", { workspaceId: "wa" });
		const b = session("b", { workspaceId: "wb" });
		const c = session("c", { workspaceId: "wc" });
		const scan = scanFromSnapshot(
			[a, b, c],
			[],
			[{ workspaceId: "wb", sessionId: "b", sessionIds: ["b"], startedAt: null }],
			[{ ...ev("turn_permission_requested", { turnId: "t", requestId: "r1", command: "yarn test" }), sessionId: "a" }],
		);
		expect([...scan.statuses]).toEqual([["b", "running"]]);
		expect(scan.needsYou.map((p) => [p.thread.sessionId, p.requestId])).toEqual([["a", "r1"]]);
		const groups = triageGroups(scan);
		expect(groups.running.map((s) => s.sessionId)).toEqual(["b"]);
		expect(groups.recent.map((s) => s.sessionId)).toEqual(["c"]);
	});
});

describe("history sweep on older desktops", () => {
	it("keeps what it knew about a thread whose history did not arrive", async () => {
		let failing = false;
		const fetchJson = async <T,>(path: string): Promise<T> => {
			if (path.startsWith("/api/v1/sessions/search")) return [session("a"), session("b")] as T;
			if (path === "/api/v1/combs") return [] as T;
			if (failing) throw new Error("timed out");
			return [ev("turn_started", { turnId: "t" })] as T;
		};
		const first = await runScan(fetchJson);
		expect(triageGroups(first.scan).running).toHaveLength(2);
		failing = true;
		const second = await runScan(fetchJson, first.scan);
		expect(triageGroups(second.scan).running.map((s) => s.sessionId)).toEqual(["a", "b"]);
	});
});

describe("live overlay", () => {
	it("moves a task in and out of running before the next read", () => {
		expect(liveRunning("turn_started")).toBe(true);
		expect(liveRunning("turn_permission_requested")).toBe(false);
		expect(liveRunning("turn_completed")).toBe(false);
		expect(liveRunning("session_started")).toBeNull();
		const scan = { sessions: [session("a"), session("b")], statuses: new Map([["b", "running" as const]]), needsYou: [] };
		const moved = withLiveOverlay(
			scan,
			new Map([
				["a", { running: true, seq: 1 }],
				["b", { running: false, seq: 2 }],
				["unknown", { running: true, seq: 3 }],
			]),
		);
		expect([...moved.statuses]).toEqual([["a", "running"]]);
		expect([...scan.statuses]).toEqual([["b", "running"]]);
	});
});
