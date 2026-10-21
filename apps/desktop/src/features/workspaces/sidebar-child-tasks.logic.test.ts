import type { Delegation } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { childTasksByWorkspace, delegatedSessionIds, withLiveTaskState } from "./sidebar-child-tasks.logic";

function delegation(overrides: Partial<Delegation>): Delegation {
	return {
		id: "d",
		parentSessionId: "parent",
		parentTurnId: null,
		childSessionId: "child",
		workspaceId: "w1",
		targetProviderId: "codex",
		targetModelId: null,
		mode: "implement",
		status: "running",
		prompt: "",
		contextPolicy: { type: "minimal" },
		budget: { turnLimit: null, timeoutSeconds: null, allowFileEdits: true, approvalPolicy: null },
		resultSummary: null,
		touchedFiles: [],
		diffSummary: null,
		validationSummary: null,
		createdAt: "2026-10-14T10:00:00Z",
		updatedAt: "2026-10-14T10:00:00Z",
		kind: "task",
		...overrides,
	} as Delegation;
}

describe("childTasksByWorkspace", () => {
	it("lists the tasks still in play under their workspace, in proposal order", () => {
		const tasks = childTasksByWorkspace([
			delegation({ id: "t2", title: "Fase 2", status: "draft", childSessionId: null, createdAt: "2026-10-14T11:00:00Z" }),
			delegation({ id: "t1", title: "Fase 1", status: "running", createdAt: "2026-10-14T10:00:00Z" }),
			delegation({ id: "t0", title: "Fase 0", status: "review_pending", childSessionId: "c0", createdAt: "2026-10-14T09:00:00Z" }),
			delegation({ id: "t3", title: "Outro", workspaceId: "w2" }),
		]);
		expect(tasks.get("w1")).toEqual([
			{ id: "t0", title: "Fase 0", state: "review", sessionId: "c0", providerId: "codex" },
			{ id: "t1", title: "Fase 1", state: "idle", sessionId: "child", providerId: "codex" },
			// A proposal opens the parent, where its card waits.
			{ id: "t2", title: "Fase 2", state: "proposed", sessionId: "parent", providerId: "codex" },
		]);
		expect(tasks.get("w2")?.map((task) => task.id)).toEqual(["t3"]);
	});

	it("leaves out plain delegations and tasks that are over", () => {
		const tasks = childTasksByWorkspace([
			delegation({ id: "helper", kind: "delegation" }),
			delegation({ id: "done", status: "completed" }),
			delegation({ id: "gone", status: "cancelled" }),
			delegation({ id: "broke", status: "failed" }),
		]);
		expect(tasks.size).toBe(0);
	});

	it("says what each open task is doing right now", () => {
		const tasks = childTasksByWorkspace([
			delegation({ id: "t1", title: "Fase 1", childSessionId: "c1" }),
			delegation({ id: "t2", title: "Fase 2", childSessionId: "c2" }),
			delegation({ id: "t3", title: "Fase 3", childSessionId: "c3" }),
			delegation({ id: "t4", title: "Fase 4", status: "draft", childSessionId: null }),
		]).get("w1")!;
		const live = { runningSessionIds: ["c1", "c2", "parent"], waitingSessionIds: ["c2"] };
		expect(withLiveTaskState(tasks, live).map((task) => task.state)).toEqual([
			"working",
			"waiting",
			// Open but between turns: not running, so it does not look busy.
			"idle",
			// A proposal opens the parent; the parent working does not make it busy.
			"proposed",
		]);
		expect(withLiveTaskState(tasks, null)).toEqual(tasks);
	});
});

describe("delegatedSessionIds", () => {
	it("collects every child conversation, plain delegations included", () => {
		const ids = delegatedSessionIds([
			delegation({ id: "review", kind: "delegation", status: "completed", childSessionId: "r1" }),
			delegation({ id: "task", childSessionId: "t1" }),
			delegation({ id: "proposal", status: "draft", childSessionId: null }),
		]);
		expect([...ids].sort()).toEqual(["r1", "t1"]);
	});
});
