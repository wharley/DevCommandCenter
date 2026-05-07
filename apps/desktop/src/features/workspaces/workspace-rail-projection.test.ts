import { describe, expect, it } from "vitest";
import {
	projectWorkspaceRailGroups,
	projectWorkspaceRepositories,
} from "./workspace-rail-projection";

describe("projectWorkspaceRailGroups", () => {
	it("groups active workspaces by project path and isolates archived rows", () => {
		const { activeGroups, archivedRows } = projectWorkspaceRailGroups(
			[
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
			],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					rootPath: "/projects/alpha",
					baseBranch: "main",
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
			],
		);

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

	it("keeps repository groups visible even when they have no active workspaces", () => {
		const { activeGroups, archivedRows } = projectWorkspaceRailGroups(
			[],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					rootPath: "/projects/alpha",
					baseBranch: "main",
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
			],
		);

		expect(activeGroups).toEqual([
			{
				id: expect.any(String),
				label: "alpha",
				sourceKey: "/projects/alpha",
				rows: [],
			},
		]);
		expect(archivedRows).toEqual([]);
	});

	it("builds repository-level options for quick workspace creation", () => {
		const repositories = projectWorkspaceRepositories([
			{
				id: "a",
				name: "Alpha",
				branch: "main",
				status: "ready",
				projectId: "alpha",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-10T10:00:00.000Z",
			},
			{
				id: "b",
				name: "Alpha hotfix",
				branch: "hotfix",
				status: "archived",
				projectId: "alpha",
				rootPath: "/projects/alpha",
				updatedAt: "2026-04-11T10:00:00.000Z",
			},
			{
				id: "c",
				name: "Beta",
				branch: "develop",
				status: "ready",
				projectId: "beta",
				rootPath: "/projects/beta",
				updatedAt: "2026-04-12T10:00:00.000Z",
			},
			{
				id: "d",
				name: "Loose Workspace",
				branch: "spike",
				status: "ready",
			},
		]);

		expect(repositories).toEqual([
			{
				sourceKey: "/projects/beta",
				label: "beta",
				projectId: "beta",
				workspaceRoot: "/projects/beta",
				branch: "develop",
				updatedAt: "2026-04-12T10:00:00.000Z",
			},
			{
				sourceKey: "/projects/alpha",
				label: "alpha",
				projectId: "alpha",
				workspaceRoot: "/projects/alpha",
				branch: "hotfix",
				updatedAt: "2026-04-11T10:00:00.000Z",
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
