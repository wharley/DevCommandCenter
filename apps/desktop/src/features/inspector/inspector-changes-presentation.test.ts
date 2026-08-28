import { describe, expect, it } from "vitest";
import {
	changeGroupBelongsToScope,
	defaultInspectorChangesScope,
	summarizeInspectorChanges,
} from "./inspector-changes-presentation";

describe("inspector changes presentation", () => {
	it("summarizes the visible scope without leaking negative counters", () => {
		expect(
			summarizeInspectorChanges([
				{ insertions: 4, deletions: 2 },
				{ insertions: 7, deletions: 0 },
				{ insertions: -1, deletions: 3 },
			]),
		).toEqual({ fileCount: 3, insertions: 11, deletions: 5 });
	});

	it("opens the working tree when it has changes", () => {
		expect(defaultInspectorChangesScope(2, 8)).toBe("working");
	});

	it("opens the branch when the working tree is clean", () => {
		expect(defaultInspectorChangesScope(0, 8)).toBe("branch");
	});

	it("keeps an empty repository on the actionable working-tree scope", () => {
		expect(defaultInspectorChangesScope(0, 0)).toBe("working");
	});

	it("keeps file previews aligned with the selected scope", () => {
		expect(changeGroupBelongsToScope("staged", "working")).toBe(true);
		expect(changeGroupBelongsToScope("unstaged", "working")).toBe(true);
		expect(changeGroupBelongsToScope("committed", "working")).toBe(false);
		expect(changeGroupBelongsToScope("committed", "branch")).toBe(true);
		expect(changeGroupBelongsToScope("unstaged", "branch")).toBe(false);
	});
});
