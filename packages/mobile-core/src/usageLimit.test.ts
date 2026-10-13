import { describe, expect, it } from "vitest";
import { attentionLabel, parseAttention } from "./attention.ts";
import { taskRequestBody } from "./catalog.ts";
import { applyEvents, createThreadState, stoppedByUsageLimit, type RawSessionEvent } from "./threadEvents.ts";
import { classifySession, type SessionSearchResult } from "./triage.ts";

const thread: SessionSearchResult = {
	sessionId: "s1",
	threadTitle: "Plano",
	snippet: null,
	providerId: "codex",
	model: null,
	workspaceName: null,
	workspaceBranch: null,
	workspaceId: "w1",
	projectId: "p1",
	updatedAt: "2026-10-12T01:30:00Z",
	archivedAt: null,
};

let sequence = 0;
const event = (kind: RawSessionEvent["kind"]): RawSessionEvent => ({
	eventId: `e${++sequence}`,
	sessionId: "s1",
	sequence,
	occurredAt: "2026-10-12T01:30:00Z",
	kind,
});

const limitedTurn = (resetsAt: string | null) => [
	event({ type: "turn_started", turnId: "t1", prompt: "implementa o plano" }),
	event({ type: "usage_limit_reached", turnId: "t1", resetsAt, autoResume: true }),
	event({ type: "turn_aborted", turnId: "t1", reason: "You've hit your usage limit." }),
];

describe("usage limit on the phone", () => {
	it("a turn stopped by the limit is a wait, not an interruption", () => {
		expect(classifySession(thread, limitedTurn("2026-10-12T03:40:00Z")).status).toBe("limited");
		const other = [
			event({ type: "turn_started", turnId: "t2", prompt: "x" }),
			event({ type: "turn_aborted", turnId: "t2", reason: "boom" }),
		];
		expect(classifySession(thread, other).status).toBe("aborted");
		// The resume starts a new turn: running again.
		const resumed = [...limitedTurn(null), event({ type: "turn_started", turnId: "t3", prompt: "Continue de onde parou." })];
		expect(classifySession(thread, resumed).status).toBe("running");
	});

	it("reads the usageLimit attention reason with the desktop's label", () => {
		const [entry] = parseAttention({
			items: [{ workspaceId: "w1", reason: "usageLimit", count: null, returnedAt: "2026-10-12T01:30:00Z", sessionId: "s1" }],
		});
		expect(entry?.reason).toBe("usageLimit");
		expect(attentionLabel(entry!)).toBe("Pausado por limite");
	});

	it("shows the stop in the thread and offers to continue until something follows", () => {
		const state = applyEvents(createThreadState(), limitedTurn(null));
		const notice = state.messages.find((m) => m.kind === "system");
		expect(notice).toMatchObject({ text: "Limite de uso atingido" });
		expect(stoppedByUsageLimit(state.messages)).toBe(true);
		// A reset reported late updates the same line.
		const later = applyEvents(state, [
			event({ type: "usage_limit_reached", turnId: "t1", resetsAt: "2026-10-12T06:40:00Z", autoResume: true }),
		]);
		const lines = later.messages.filter((m) => m.kind === "system");
		expect(lines).toHaveLength(1);
		expect((lines[0] as { text: string }).text).toMatch(/^Limite de uso atingido · volta às \d\d:\d\d$/);
		const resumed = applyEvents(later, [
			event({ type: "turn_started", turnId: "t2", prompt: "Continue de onde parou." }),
			event({ type: "usage_limit_resumed", automatic: true }),
		]);
		expect(stoppedByUsageLimit(resumed.messages)).toBe(false);
		expect(resumed.messages.at(-1)).toMatchObject({ text: "Retomado sozinho depois do limite" });
	});

	it("sends the task's choice only when the person made one", () => {
		const draft = {
			requestId: "r",
			repositoryId: "repo",
			prompt: "faz",
			providerId: "codex",
			model: null,
			worktree: true,
		};
		expect(taskRequestBody(draft)).not.toHaveProperty("autoResume");
		expect(taskRequestBody({ ...draft, autoResume: true })).toMatchObject({ autoResume: true });
		expect(taskRequestBody({ ...draft, autoResume: false })).toMatchObject({ autoResume: false });
	});
});
