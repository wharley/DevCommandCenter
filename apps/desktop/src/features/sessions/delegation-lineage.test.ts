import { describe, expect, it } from "vitest";
import type { Delegation, WorkspaceSessionSummary } from "@dcc/contracts";
import {
	childDelegationsOf,
	delegationInstruction,
	isDelegationActive,
	nestSessionsByLineage,
	parentDelegationOf,
	relativeTimeLabel,
} from "./delegation-lineage";

function delegation(overrides: Partial<Delegation>): Delegation {
	return {
		id: "d",
		parentSessionId: "parent",
		parentTurnId: null,
		childSessionId: "child",
		workspaceId: "w",
		targetProviderId: "codex",
		targetModelId: null,
		mode: "review",
		status: "running",
		prompt: "Delegated review task\n\nInstruction:\nReview the diff.\n\nGit context:\n- x",
		contextPolicy: { type: "review_current_diff" },
		budget: { turnLimit: 1, timeoutSeconds: 900, allowFileEdits: false, approvalPolicy: "read_only" },
		resultSummary: null,
		touchedFiles: [],
		diffSummary: null,
		validationSummary: null,
		createdAt: "2026-10-05T10:00:00Z",
		updatedAt: "2026-10-05T10:00:00Z",
		origin: "agent",
		instruction: null,
		startedAt: null,
		...overrides,
	} as Delegation;
}

const session = (id: string) => ({ session: { id }, thread: { title: id } }) as unknown as WorkspaceSessionSummary;

describe("delegation lineage", () => {
	it("lists a session's children newest first and finds a child's parent", () => {
		const older = delegation({ id: "a", childSessionId: "c1", createdAt: "2026-10-05T09:00:00Z" });
		const newer = delegation({ id: "b", childSessionId: "c2", createdAt: "2026-10-05T11:00:00Z" });
		const other = delegation({ id: "c", parentSessionId: "someone-else", childSessionId: "c3" });
		expect(childDelegationsOf([older, newer, other], "parent").map((d) => d.id)).toEqual(["b", "a"]);
		expect(childDelegationsOf([older], null)).toEqual([]);
		expect(parentDelegationOf([older, newer], "c2")?.id).toBe("b");
		expect(parentDelegationOf([older], "parent")).toBeNull();
	});

	it("treats only unfinished delegations as active", () => {
		expect(isDelegationActive(delegation({ status: "running" }))).toBe(true);
		expect(isDelegationActive(delegation({ status: "queued" }))).toBe(true);
		expect(isDelegationActive(delegation({ status: "review_pending" }))).toBe(false);
		expect(isDelegationActive(delegation({ status: "completed" }))).toBe(false);
	});

	it("nests delegated children right under their parent session", () => {
		const rows = nestSessionsByLineage(
			[session("child"), session("other"), session("parent")],
			[delegation({ childSessionId: "child" })],
		);
		expect(rows.map((row) => [row.summary.session.id, row.depth])).toEqual([
			["other", 0],
			["parent", 0],
			["child", 1],
		]);
	});

	it("keeps a child top-level when its parent is not listed", () => {
		const rows = nestSessionsByLineage([session("child")], [delegation({ childSessionId: "child" })]);
		expect(rows).toEqual([{ summary: session("child"), depth: 0 }]);
	});

	it("shows the instruction as written, falling back to the prompt block", () => {
		expect(delegationInstruction(delegation({ instruction: "  Check the tests " }))).toBe("Check the tests");
		expect(delegationInstruction(delegation({}))).toBe("Review the diff.");
	});

	it("formats past timestamps relative to now", () => {
		const now = Date.parse("2026-10-05T10:05:00Z");
		expect(relativeTimeLabel("2026-10-05T10:00:00Z", "en", now)).toBe("5 min. ago");
		expect(relativeTimeLabel(null, "en", now)).toBeNull();
	});
});
