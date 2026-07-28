import type { WorkspaceGitConflictStateOutput } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import { isMergeConflictResolutionReady } from "./merge-conflict-readiness";

function state(resultText: string | null): WorkspaceGitConflictStateOutput {
	return {
		operation: "merge",
		currentBranch: "feature/example",
		incomingRef: "origin/main",
		conflicts: [
			{
				path: "src/example.ts",
				kind: "both-modified",
				base: {
					exists: true,
					binary: false,
					truncated: false,
					byteCount: 4,
					mode: "100644",
					text: "base",
				},
				current: {
					exists: true,
					binary: false,
					truncated: false,
					byteCount: 7,
					mode: "100644",
					text: "current",
				},
				incoming: {
					exists: true,
					binary: false,
					truncated: false,
					byteCount: 8,
					mode: "100644",
					text: "incoming",
				},
				result: {
					exists: true,
					binary: false,
					truncated: false,
					byteCount: resultText?.length ?? 0,
					mode: null,
					text: resultText,
				},
			},
		],
	};
}

describe("isMergeConflictResolutionReady", () => {
	it("recognizes a marker-free text resolution that Git still reports as unmerged", () => {
		expect(isMergeConflictResolutionReady(state("current + incoming\n"))).toBe(true);
	});

	it("keeps results with conflict markers in the resolver", () => {
		expect(
			isMergeConflictResolutionReady(
				state("<<<<<<< HEAD\ncurrent\n=======\nincoming\n>>>>>>> main\n"),
			),
		).toBe(false);
	});

	it("does not infer intent for unavailable result text", () => {
		expect(isMergeConflictResolutionReady(state(null))).toBe(false);
	});

	it("keeps modify/delete conflicts in the explicit file resolver", () => {
		const conflictState = state("kept content\n");
		conflictState.conflicts[0]!.kind = "deleted-by-incoming";

		expect(isMergeConflictResolutionReady(conflictState)).toBe(false);
	});
});
