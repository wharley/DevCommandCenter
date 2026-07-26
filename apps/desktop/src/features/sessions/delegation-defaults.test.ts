import { describe, expect, it } from "vitest";
import { resolveDelegationDefaults } from "./delegation-defaults";

describe("resolveDelegationDefaults", () => {
	it("uses implement + full reanchor when file edits are allowed", () => {
		expect(
			resolveDelegationDefaults({
				allowFileEdits: true,
				hasWorkingTreeChanges: false,
			}),
		).toEqual({ mode: "implement", contextPolicy: { type: "full_reanchor" } });
	});

	it("keeps implement + full reanchor even if the tree is dirty", () => {
		// The clean-tree preflight owns that failure; the derivation must not flip
		// the mode the user explicitly asked for.
		expect(
			resolveDelegationDefaults({
				allowFileEdits: true,
				hasWorkingTreeChanges: true,
			}),
		).toEqual({ mode: "implement", contextPolicy: { type: "full_reanchor" } });
	});

	it("reviews the current diff when read-only work has changes to look at", () => {
		expect(
			resolveDelegationDefaults({
				allowFileEdits: false,
				hasWorkingTreeChanges: true,
			}),
		).toEqual({ mode: "review", contextPolicy: { type: "review_current_diff" } });
	});

	it("falls back to explain + minimal when there is no diff to review", () => {
		expect(
			resolveDelegationDefaults({
				allowFileEdits: false,
				hasWorkingTreeChanges: false,
			}),
		).toEqual({ mode: "explain", contextPolicy: { type: "minimal" } });
	});
});
