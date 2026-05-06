import { describe, expect, it } from "vitest";
import { removeWorkspacesFromList } from "./use-workspaces";

describe("removeWorkspacesFromList", () => {
	it("keeps the current selection when it is not removed", () => {
		const result = removeWorkspacesFromList(
			[
				{ id: "a", name: "Alpha", branch: "main", status: "ready" },
				{ id: "b", name: "Beta", branch: "feat/beta", status: "ready" },
			],
			["a"],
			"b",
		);

		expect(result.workspaceList.map((workspace) => workspace.id)).toEqual(["b"]);
		expect(result.selectedWorkspaceId).toBe("b");
	});

	it("falls back to the first remaining workspace when the selected one is removed", () => {
		const result = removeWorkspacesFromList(
			[
				{ id: "a", name: "Alpha", branch: "main", status: "ready" },
				{ id: "b", name: "Beta", branch: "feat/beta", status: "ready" },
				{ id: "c", name: "Gamma", branch: "feat/gamma", status: "archived" },
			],
			["b", "c"],
			"b",
		);

		expect(result.workspaceList.map((workspace) => workspace.id)).toEqual(["a"]);
		expect(result.selectedWorkspaceId).toBe("a");
	});

	it("clears the selection when no workspaces remain", () => {
		const result = removeWorkspacesFromList(
			[{ id: "a", name: "Alpha", branch: "main", status: "ready" }],
			["a"],
			"a",
		);

		expect(result.workspaceList).toEqual([]);
		expect(result.selectedWorkspaceId).toBeNull();
	});
});
