import { describe, expect, it } from "vitest";
import { removeWorkspacesFromList, workspaceMutationIds } from "./use-workspaces";

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

describe("workspaceMutationIds", () => {
	it("returns every member when removing a bundle", () => {
		expect(
			workspaceMutationIds(
				{
					id: "primary",
					name: "Task",
					branch: "dcc/task",
					status: "ready",
					bundleId: "bundle-1",
					memberWorkspaceIds: ["primary", "api", "web"],
				},
				"primary",
			),
		).toEqual(["primary", "api", "web"]);
	});

	it("keeps single-workspace deletion unchanged", () => {
		expect(workspaceMutationIds(undefined, "single")).toEqual(["single"]);
	});
});
