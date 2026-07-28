import type { WorkspaceDeliveryFailureSnapshot } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { buildDeliveryFailureComposerPrompt } from "./delivery-failure-format";

function failure(
	overrides: Partial<WorkspaceDeliveryFailureSnapshot> = {},
): WorkspaceDeliveryFailureSnapshot {
	return {
		attemptToken: "attempt-1",
		workspaceRoot: "/tmp/dcc",
		branch: "feature/recovery",
		headSha: "abc123",
		operation: "push",
		classification: "authentication",
		remote: "origin",
		operationTarget: null,
		pushTarget: {
			remote: "origin",
			branch: "feature/recovery",
			url: null,
		},
		output: "authentication failed",
		outputTruncated: false,
		changedFiles: ["src/main.ts"],
		changedFilesTruncated: false,
		externalUrl: null,
		availableActions: ["retry", "send-to-agent"],
		createdAt: "2026-07-24T12:00:00Z",
		...overrides,
	};
}

describe("buildDeliveryFailureComposerPrompt", () => {
	it("includes the captured attempt and conservative recovery constraints", () => {
		const prompt = buildDeliveryFailureComposerPrompt(failure());

		expect(prompt).toContain("Attempt token: attempt-1");
		expect(prompt).toContain("Commit: abc123");
		expect(prompt).toContain("Do not bypass Git hooks, force-push, or merge automatically.");
		expect(prompt).toContain(
			"do not stage, commit, or push. Leave the explicit completion checkpoint to the DCC Inspector.",
		);
		expect(prompt).toContain("- src/main.ts");
	});

	it("bounds output and changed paths before sending context to the agent", () => {
		const prompt = buildDeliveryFailureComposerPrompt(
			failure({
				output: "x".repeat(20_000),
				changedFiles: Array.from({ length: 100 }, (_, index) => `src/${index}.ts`),
				changedFilesTruncated: true,
			}),
		);

		expect(prompt.length).toBeLessThan(10_000);
		expect(prompt).toContain("[truncated for agent context]");
		expect(prompt).toContain("50 more path(s) omitted");
		expect(prompt).not.toContain("src/99.ts");
	});
});
