import { describe, expect, it } from "vitest";
import {
	shouldAutoCompleteMergedWorkspace,
	shouldCollapseContextualInspector,
} from "./inspector-presentation";

describe("shouldCollapseContextualInspector", () => {
	it("collapses an inspector opened for a contextual workflow", () => {
		expect(shouldCollapseContextualInspector("contextual", false)).toBe(true);
	});

	it("keeps an inspector the user pinned open", () => {
		expect(shouldCollapseContextualInspector("pinned", false)).toBe(false);
	});

	it("does not request another collapse when it is already closed", () => {
		expect(shouldCollapseContextualInspector("contextual", true)).toBe(false);
	});
});

describe("shouldAutoCompleteMergedWorkspace", () => {
	it("completes a worktree task once its change request is merged", () => {
		expect(
			shouldAutoCompleteMergedWorkspace({
				commitMode: "merged",
				workspaceStatus: "ready",
				hasOwnWorktree: true,
			}),
		).toBe(true);
	});

	it("keeps a task without a worktree open when its branch has an old merged request", () => {
		expect(
			shouldAutoCompleteMergedWorkspace({
				commitMode: "merged",
				workspaceStatus: "ready",
				hasOwnWorktree: false,
			}),
		).toBe(false);
	});

	it("does not complete a task twice", () => {
		expect(
			shouldAutoCompleteMergedWorkspace({
				commitMode: "merged",
				workspaceStatus: "completed",
				hasOwnWorktree: true,
			}),
		).toBe(false);
	});
});
