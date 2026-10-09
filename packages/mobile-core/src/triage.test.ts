import { describe, expect, it } from "vitest";
import type { RawSessionEvent } from "./threadEvents.ts";
import {
	classifySession,
	liveSessions,
	runScan,
	triageGroups,
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
