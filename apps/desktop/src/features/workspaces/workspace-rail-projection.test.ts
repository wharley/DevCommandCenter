import { describe, expect, it } from "vitest";
import { projectWorkspaceRailGroups } from "./workspace-rail-projection";

describe("projectWorkspaceRailGroups", () => {
	it("groups active workspaces by project path and isolates archived rows", () => {
		const { activeGroups, archivedRows } = projectWorkspaceRailGroups([
			{
				id: "a",
				name: "Alpha",
				branch: "main",
				status: "ready",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-10T10:00:00.000Z",
			},
			{
				id: "b",
				name: "Alpha hotfix",
				branch: "hotfix",
				status: "ready",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-11T10:00:00.000Z",
			},
			{
				id: "c",
				name: "Archived Spike",
				branch: "spike",
				status: "archived",
				projectId: "project-spike",
			},
		]);

		expect(activeGroups).toHaveLength(1);
		expect(activeGroups[0]).toMatchObject({
			label: "alpha",
			rows: [
				{ id: "b", name: "Alpha hotfix" },
				{ id: "a", name: "Alpha" },
			],
		});
		expect(archivedRows).toEqual([
			{
				id: "c",
				name: "Archived Spike",
				branch: "spike",
				status: "archived",
				projectId: "project-spike",
			},
		]);
	});

	it("falls back to workspace identity when path and project id are missing", () => {
		const { activeGroups } = projectWorkspaceRailGroups([
			{
				id: "z",
				name: "Loose Workspace",
				branch: "feat/loose",
				status: "ready",
			},
		]);

		expect(activeGroups).toHaveLength(1);
		expect(activeGroups[0]).toMatchObject({
			label: "Loose Workspace",
			rows: [{ id: "z", name: "Loose Workspace" }],
		});
	});
});
