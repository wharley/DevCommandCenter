import { describe, expect, it, vi } from "vitest";
import type {
	ExecuteGuardedUndoOutput,
	PrepareGuardedUndoOutput,
	TurnReviewSummary,
} from "@dcc/contracts";
import {
	executeTurnUndo,
	planTurnUndo,
	prepareTurnUndo,
	type PreparedTurnUndo,
} from "./turn-undo-across-projects.logic";

function review(
	snapshotId: string,
	options: { files?: number; undo?: "eligible" | "ineligible"; active?: boolean } = {},
): TurnReviewSummary {
	return {
		snapshotId,
		sessionId: "session",
		turnId: "turn",
		workspaceId: snapshotId,
		state: "completed",
		compatibility: "matches_result",
		baseFingerprint: null,
		resultFingerprint: null,
		files: Array.from({ length: options.files ?? 1 }, (_, index) => ({
			path: `file-${index}.ts`,
		})) as TurnReviewSummary["files"],
		insertions: 1,
		deletions: 0,
		diffTruncated: false,
		excludedPreexistingUntrackedCount: 0,
		observedValidations: [],
		turnOutcome: "completed",
		outcomeReason: null,
		error: null,
		completedAt: null,
		guardedUndo: { state: options.undo ?? "eligible" } as TurnReviewSummary["guardedUndo"],
		activeUndo: options.active
			? ({ status: "running" } as TurnReviewSummary["activeUndo"])
			: null,
	};
}

function ready(snapshotId: string): PrepareGuardedUndoOutput {
	return {
		status: "ready",
		snapshotId,
		previewToken: `token-${snapshotId}`,
		expiresAt: "2026-10-13T12:00:00Z",
		fileCount: 1,
		totalBytes: 10,
		files: [],
		unrelatedPathsAreNotTargets: true,
	};
}

function prepared(snapshotId: string): PreparedTurnUndo {
	return {
		workspaceId: snapshotId,
		name: snapshotId.toUpperCase(),
		snapshotId,
		preview: ready(snapshotId) as PreparedTurnUndo["preview"],
	};
}

describe("planTurnUndo", () => {
	it("targets every changed project Undo can restore and lists the rest", () => {
		const plan = planTurnUndo([
			{ workspaceId: "dcc", name: "DCC", review: review("dcc") },
			{ workspaceId: "lp", name: "LP", review: review("lp", { undo: "ineligible" }) },
			{ workspaceId: "api", name: "API", review: review("api", { files: 0 }) },
			{ workspaceId: "web", name: "Web", review: null },
		]);
		expect(plan.targets).toEqual([{ workspaceId: "dcc", name: "DCC", snapshotId: "dcc" }]);
		expect(plan.uncovered).toEqual([{ workspaceId: "lp", name: "LP" }]);
	});

	it("leaves a project with an Undo already running out of the targets", () => {
		const plan = planTurnUndo([
			{ workspaceId: "dcc", name: "DCC", review: review("dcc", { active: true }) },
		]);
		expect(plan.targets).toEqual([]);
		expect(plan.uncovered).toEqual([{ workspaceId: "dcc", name: "DCC" }]);
	});
});

describe("prepareTurnUndo", () => {
	const targets = [
		{ workspaceId: "dcc", name: "DCC", snapshotId: "dcc" },
		{ workspaceId: "lp", name: "LP", snapshotId: "lp" },
	];

	it("is ready only when every project is ready", async () => {
		const result = await prepareTurnUndo(targets, async (id) => ready(id));
		expect(result.status).toBe("ready");
		expect(result.status === "ready" && result.prepared.map((p) => p.workspaceId)).toEqual([
			"dcc",
			"lp",
		]);
	});

	it("stops at the first project that is not free", async () => {
		const prepare = vi.fn(async (id: string): Promise<PrepareGuardedUndoOutput> =>
			id === "dcc"
				? { status: "blocked", snapshotId: id, reasonCode: "head_changed", detailsAvailable: false }
				: ready(id),
		);
		const result = await prepareTurnUndo(targets, prepare);
		expect(result).toEqual({
			status: "blocked",
			target: targets[0],
			result: "blocked",
			reasonCode: "head_changed",
		});
		expect(prepare).toHaveBeenCalledTimes(1);
	});

	it("treats a preview that may touch unrelated paths as unavailable", async () => {
		const result = await prepareTurnUndo(targets.slice(0, 1), async (id) => ({
			...(ready(id) as Extract<PrepareGuardedUndoOutput, { status: "ready" }>),
			unrelatedPathsAreNotTargets: false,
		}));
		expect(result).toMatchObject({ status: "blocked", result: "unavailable" });
	});
});

describe("executeTurnUndo", () => {
	it("restores every project in order", async () => {
		const execute = vi.fn(
			async (_token: string): Promise<ExecuteGuardedUndoOutput> => ({
				status: "completed",
				operationId: null,
				reasonCode: null,
			}),
		);
		const results = await executeTurnUndo([prepared("dcc"), prepared("lp")], execute);
		expect(execute.mock.calls.map(([token]) => token)).toEqual(["token-dcc", "token-lp"]);
		expect(results.map((result) => result.status)).toEqual(["completed", "completed"]);
	});

	it("stops after a project that does not complete and reports the rest", async () => {
		const execute = vi.fn(
			async (token: string): Promise<ExecuteGuardedUndoOutput> =>
				token === "token-lp"
					? { status: "rolled_back", operationId: null, reasonCode: "file_changed" }
					: { status: "completed", operationId: null, reasonCode: null },
		);
		const results = await executeTurnUndo(
			[prepared("dcc"), prepared("lp"), prepared("api")],
			execute,
		);
		expect(results).toEqual([
			{ workspaceId: "dcc", name: "DCC", status: "completed", reasonCode: null },
			{ workspaceId: "lp", name: "LP", status: "rolled_back", reasonCode: "file_changed" },
			{ workspaceId: "api", name: "API", status: "not_attempted", reasonCode: null },
		]);
		expect(execute).toHaveBeenCalledTimes(2);
	});

	it("reports a request that throws as failed and stops", async () => {
		const results = await executeTurnUndo([prepared("dcc"), prepared("lp")], async () => {
			throw new Error("offline");
		});
		expect(results.map((result) => result.status)).toEqual(["failed", "not_attempted"]);
	});
});
