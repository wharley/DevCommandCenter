import { describe, expect, it, vi } from "vitest";
import type { Delegation, ProviderCatalog } from "@dcc/contracts";
import {
	buildDelegationResultTurn,
	delegationResultTurnFromRecord,
	extractDelegationInstruction,
} from "./delegation-result-turn";
import { deliverDelegationResultToParent } from "./deliver-delegation-result";

const PROMPT = [
	"Delegated review task from Dev Command Center.",
	"",
	"Workspace:",
	"- Name: dcc",
	"",
	"Instruction:",
	"Review the sidebar changes for regressions.",
	"Focus on the virtual list.",
	"",
	"Git context:",
	"- Current branch: main",
].join("\n");

function delegation(overrides: Partial<Delegation> = {}): Delegation {
	return {
		id: "delegation-1",
		parentSessionId: "session-1",
		parentTurnId: null,
		childSessionId: "session-2",
		workspaceId: "workspace-1",
		targetProviderId: "codex",
		targetModelId: null,
		mode: "review",
		status: "completed",
		prompt: PROMPT,
		contextPolicy: { type: "review_current_diff" },
		budget: { turnLimit: 1, timeoutSeconds: 600, allowFileEdits: false },
		resultSummary: "No regressions found.",
		diffSummary: null,
		validationSummary: null,
		createdAt: "2026-10-05T00:00:00Z",
		updatedAt: "2026-10-05T00:00:00Z",
		...overrides,
	} as Delegation;
}

const providers = [{ id: "codex", label: "Codex" }] as unknown as ProviderCatalog["providers"];

describe("buildDelegationResultTurn", () => {
	it("hands back the task, the result and the validation", () => {
		const text = buildDelegationResultTurn({
			mode: "review",
			providerLabel: "Codex",
			modelLabel: "GPT-5.6",
			instruction: "Review the diff.",
			status: "completed",
			summary: "Looks good.",
			validationSummary: "Validation-like commands: yarn test",
		});
		expect(text.startsWith("[DCC] Delegated review task finished — Codex (GPT-5.6).")).toBe(true);
		expect(text).toContain("Task you delegated:\nReview the diff.");
		expect(text).toContain("Result:\nLooks good.");
		expect(text).toContain("Validation:\nValidation-like commands: yarn test");
		expect(text).not.toContain("NOT applied");
	});

	it("warns that implementation edits are not applied yet", () => {
		const text = buildDelegationResultTurn({
			mode: "implement",
			providerLabel: "Gemini",
			instruction: "Add the button.",
			status: "review_pending",
			summary: "Added it.",
			touchedFiles: Array.from({ length: 22 }, (_, index) => `src/file-${index}.ts`),
		});
		expect(text).toContain("Files touched (22):");
		expect(text).toContain("- … 2 more");
		expect(text).toContain("NOT applied to your workspace yet");
	});

	it("reports failures with the reason and no result block", () => {
		const text = buildDelegationResultTurn({
			mode: "review",
			providerLabel: "Codex",
			instruction: "Review.",
			status: "failed",
			failureReason: "Provider timed out.",
		});
		expect(text).toContain("[DCC] Delegated review task failed — Codex.");
		expect(text).toContain("Failure: Provider timed out.");
		expect(text).not.toContain("Result:");
	});

	it("clips a long result", () => {
		const text = buildDelegationResultTurn({
			mode: "explain",
			providerLabel: "Codex",
			instruction: "Explain.",
			status: "completed",
			summary: "x".repeat(10_000),
		});
		expect(text).toContain("[truncated]");
		expect(text.length).toBeLessThan(5_000);
	});
});

describe("extractDelegationInstruction", () => {
	it("reads only the instruction block", () => {
		expect(extractDelegationInstruction(PROMPT)).toBe(
			"Review the sidebar changes for regressions.\nFocus on the virtual list.",
		);
	});

	it("keeps a plain instruction as is", () => {
		expect(extractDelegationInstruction("  Just review it.  ")).toBe("Just review it.");
	});
});

describe("delegationResultTurnFromRecord", () => {
	it("needs a finished delegation", () => {
		expect(delegationResultTurnFromRecord(delegation({ status: "running" }), providers)).toBeNull();
	});

	it("uses the failure reason from the timeline when given", () => {
		const text = delegationResultTurnFromRecord(
			delegation({ status: "failed", resultSummary: null }),
			providers,
			"Child session failed.",
		);
		expect(text).toContain("Failure: Child session failed.");
		expect(text).toContain("Task you delegated:\nReview the sidebar changes for regressions.");
	});
});

describe("deliverDelegationResultToParent", () => {
	const input = { parentSessionId: "parent", prompt: "[DCC] result", toolInstructions: null };

	it("queues behind the parent's open turn", async () => {
		const api = { queueTurn: vi.fn().mockResolvedValue({}), sendTurn: vi.fn() };
		await expect(deliverDelegationResultToParent(input, api)).resolves.toBe("queued");
		expect(api.queueTurn).toHaveBeenCalledWith({
			turn: expect.objectContaining({ sessionId: "parent", prompt: "[DCC] result" }),
		});
		expect(api.sendTurn).not.toHaveBeenCalled();
	});

	it("starts a new turn when the parent is idle", async () => {
		const api = {
			queueTurn: vi
				.fn()
				.mockRejectedValue(new Error("follow-ups can only be queued while a turn is active")),
			sendTurn: vi.fn().mockResolvedValue({}),
		};
		await expect(deliverDelegationResultToParent(input, api)).resolves.toBe("sent");
		expect(api.sendTurn).toHaveBeenCalledWith(
			expect.objectContaining({ sessionId: "parent", prompt: "[DCC] result" }),
		);
	});
});
