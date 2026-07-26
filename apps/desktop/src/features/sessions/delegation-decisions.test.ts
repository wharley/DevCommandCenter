import { describe, expect, it } from "vitest";
import type { Delegation, ProviderCatalog } from "@dcc/contracts";
import {
	canRerunDelegation,
	describeDelegation,
	rerunMode,
	rerunTargets,
} from "./delegation-decisions";

function delegation(overrides: Partial<Delegation> = {}): Delegation {
	return {
		id: "delegation-1",
		parentSessionId: "session-1",
		parentTurnId: null,
		childSessionId: "session-2",
		workspaceId: "workspace-1",
		targetProviderId: "codex",
		mode: "review",
		status: "completed",
		prompt: "Delegated review task from Dev Command Center.",
		contextPolicy: { type: "review_current_diff" },
		budget: { turnLimit: 1, timeoutSeconds: 600, allowFileEdits: false },
		resultSummary: null,
		diffSummary: null,
		validationSummary: null,
		createdAt: "2026-07-26T00:00:00Z",
		updatedAt: "2026-07-26T00:00:00Z",
		...overrides,
	} as Delegation;
}

const providers = [
	{ id: "codex", label: "Codex" },
	{ id: "gemini", label: "Gemini" },
] as unknown as ProviderCatalog["providers"];

describe("describeDelegation", () => {
	it("reads the decisions off the record", () => {
		expect(describeDelegation(delegation(), providers)).toEqual({
			mode: "review",
			providerId: "codex",
			providerLabel: "Codex",
			contextPolicy: "review_current_diff",
			allowFileEdits: false,
		});
	});

	it("falls back to the provider id when the catalog no longer has it", () => {
		expect(
			describeDelegation(delegation({ targetProviderId: "retired" }), providers)
				.providerLabel,
		).toBe("retired");
	});
});

describe("canRerunDelegation", () => {
	it("allows rerunning finished read-only delegations", () => {
		expect(canRerunDelegation(delegation({ status: "completed" }))).toBe(true);
		expect(canRerunDelegation(delegation({ status: "failed" }))).toBe(true);
		expect(canRerunDelegation(delegation({ status: "cancelled" }))).toBe(true);
	});

	it("refuses while the delegation is still in flight", () => {
		expect(canRerunDelegation(delegation({ status: "running" }))).toBe(false);
		expect(canRerunDelegation(delegation({ status: "queued" }))).toBe(false);
		expect(canRerunDelegation(delegation({ status: "review_pending" }))).toBe(
			false,
		);
	});

	it("refuses implementation delegations, whose prompt pins a child worktree", () => {
		expect(canRerunDelegation(delegation({ mode: "implement" }))).toBe(false);
		expect(
			canRerunDelegation(
				delegation({
					budget: { turnLimit: 1, timeoutSeconds: 600, allowFileEdits: true },
				}),
			),
		).toBe(false);
	});

	it("refuses when there is no record yet", () => {
		expect(canRerunDelegation(null)).toBe(false);
	});
});

describe("rerunMode", () => {
	it("keeps explain and collapses every other read-only mode onto review", () => {
		expect(rerunMode(delegation({ mode: "explain" }))).toBe("explain");
		expect(rerunMode(delegation({ mode: "review" }))).toBe("review");
		expect(rerunMode(delegation({ mode: "test" }))).toBe("review");
		expect(rerunMode(delegation({ mode: "research" }))).toBe("review");
	});
});

describe("rerunTargets", () => {
	it("drops the agent that already ran the delegation", () => {
		expect(rerunTargets(delegation(), providers).map((p) => p.id)).toEqual([
			"gemini",
		]);
	});
});
