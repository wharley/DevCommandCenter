import { describe, expect, it } from "vitest";
import {
	projectWorkspaceRailGroups,
	projectWorkspaceRepositories,
} from "./workspace-rail-projection";

describe("projectWorkspaceRailGroups", () => {
	it("groups active workspaces by project path and separates waiting and completed rows", () => {
		const { activeGroups, waitingRows, completedRows } = projectWorkspaceRailGroups(
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
					name: "Waiting Spike",
					branch: "spike",
					status: "archived",
					projectId: "project-spike",
				},
				{
					id: "d",
					name: "Completed Feature",
					branch: "feature/done",
					status: "completed",
					projectId: "project-done",
				},
			],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					displayName: "Customer Portal",
					icon: "rocket",
					color: "violet",
					rootPath: "/projects/alpha",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
					createdAt: "2026-04-10T10:00:00.000Z",
					updatedAt: "2026-04-11T10:00:00.000Z",
				},
			],
		);

		expect(activeGroups).toHaveLength(1);
		expect(activeGroups[0]).toMatchObject({
			label: "Customer Portal",
			rows: [
				{ id: "b", name: "Alpha hotfix" },
				{ id: "a", name: "Alpha" },
			],
		});
		expect(waitingRows).toEqual([
			{
				id: "c",
				name: "Waiting Spike",
				branch: "spike",
				status: "archived",
				projectId: "project-spike",
			},
		]);
		expect(completedRows).toEqual([
			{
				id: "d",
				name: "Completed Feature",
				branch: "feature/done",
				status: "completed",
				projectId: "project-done",
			},
		]);
	});

	it("keeps repository groups visible even when they have no active workspaces", () => {
		const { activeGroups, waitingRows, completedRows } = projectWorkspaceRailGroups(
			[],
			[
				{
					id: "/projects/alpha",
					projectId: "alpha",
					name: "alpha",
					displayName: null,
					icon: null,
					color: null,
					rootPath: "/projects/alpha",
					baseBranch: "main",
					remote: null,
					remoteUrl: null,
					forgeProvider: null,
					forgeLogin: null,
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
		expect(waitingRows).toEqual([]);
		expect(completedRows).toEqual([]);
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
