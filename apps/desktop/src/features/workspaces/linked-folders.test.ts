import { describe, expect, it } from "vitest";
import type { Repository, WorkspaceBundleSummary } from "@dcc/contracts";
import {
	additionalWorkspaceIdsOf,
	linkedFoldersOf,
	linkedTaskProjects,
} from "./linked-folders";

const repositories = [
	{
		id: "repo-dcc",
		projectId: "dcc",
		name: "dcc",
		displayName: "DevCommandCenter",
		rootPath: "/code/dcc",
		baseBranch: "main",
	},
	{
		id: "repo-lp",
		projectId: "lp",
		name: "lp",
		displayName: "DCC LP",
		rootPath: "/code/lp",
		baseBranch: "develop",
	},
] as Repository[];

describe("linkedFoldersOf", () => {
	it("finds the project by its root, trailing slash or not", () => {
		const linked = { "repo-dcc": ["/code/lp"] };
		expect(linkedFoldersOf("/code/dcc/", repositories, linked)).toEqual(["/code/lp"]);
		expect(linkedFoldersOf("/code/other", repositories, linked)).toEqual([]);
	});
});

describe("linkedTaskProjects", () => {
	it("puts the project first and every linked folder after it, in one mode", () => {
		expect(
			linkedTaskProjects({
				primary: { projectId: "dcc", workspaceRoot: "/code/dcc", baseBranch: "main" },
				linkedFolders: ["/code/lp", "/code/docs-site"],
				repositories,
				isolationMode: "localDirect",
			}),
		).toEqual([
			{
				projectId: "dcc",
				workspaceRoot: "/code/dcc",
				baseBranch: "main",
				name: "DevCommandCenter",
				isolationMode: "localDirect",
			},
			{
				projectId: "lp",
				workspaceRoot: "/code/lp",
				baseBranch: "develop",
				name: "DCC LP",
				isolationMode: "localDirect",
			},
			{
				projectId: "docs-site",
				workspaceRoot: "/code/docs-site",
				baseBranch: "",
				name: "docs-site",
				isolationMode: "localDirect",
			},
		]);
	});

	it("keeps worktrees as the default mode", () => {
		const projects = linkedTaskProjects({
			primary: { projectId: "dcc", workspaceRoot: "/code/dcc", baseBranch: "main" },
			linkedFolders: ["/code/lp"],
			repositories,
			isolationMode: "protectedWorktree",
		});
		expect(projects.map((project) => project.isolationMode)).toEqual([null, null]);
	});
});

describe("additionalWorkspaceIdsOf", () => {
	it("lists the other members of the task whose primary is the workspace", () => {
		const bundles = [
			{
				bundle: { primaryWorkspaceId: "w-dcc" },
				members: [{ workspaceId: "w-dcc" }, { workspaceId: "w-lp" }],
			},
		] as WorkspaceBundleSummary[];
		expect(additionalWorkspaceIdsOf("w-dcc", bundles)).toEqual(["w-lp"]);
		expect(additionalWorkspaceIdsOf("w-lp", bundles)).toEqual([]);
	});
});
