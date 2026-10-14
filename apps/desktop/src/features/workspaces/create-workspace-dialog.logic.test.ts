import { describe, expect, it } from "vitest";
import {
	includePickedRepository,
	inferProjectIdFromWorkspaceRoot,
	initialTaskRepository,
	initialWorkspaceStart,
	isBranchWorkspaceSource,
	LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY,
	linkedFolderOptionId,
	orderMultiProjectSelection,
	promoteMultiProjectPrimary,
	selectMultiProject,
	withLinkedFolderOptions,
	readLastTaskRepositoryRoot,
	rememberLastTaskRepositoryRoot,
	repositoryNameFromWorkspaceRoot,
} from "./create-workspace-dialog.logic";

describe("inferProjectIdFromWorkspaceRoot", () => {
	it("uses the last folder name on unix paths", () => {
		expect(
			inferProjectIdFromWorkspaceRoot("/Users/dev/workspaces/My App"),
		).toBe("my-app");
	});

	it("uses the last folder name on windows paths", () => {
		expect(
			inferProjectIdFromWorkspaceRoot("C:\\repos\\DCC Demo\\"),
		).toBe("dcc-demo");
	});

	it("falls back to project when it cannot infer a clean id", () => {
		expect(inferProjectIdFromWorkspaceRoot("/")).toBe("project");
		expect(inferProjectIdFromWorkspaceRoot("   ")).toBe("project");
	});
});

describe("includePickedRepository", () => {
	const tracked = { id: "tracked", rootPath: "/projects/dcc" };

	it("adds an untracked folder as the first selectable project", () => {
		const picked = { id: "picked", rootPath: "/projects/new-app" };
		expect(includePickedRepository([tracked], picked)).toEqual([picked, tracked]);
	});

	it("does not duplicate an already tracked folder", () => {
		const picked = { id: "picked", rootPath: "/projects/dcc/" };
		expect(includePickedRepository([tracked], picked)).toEqual([tracked]);
	});

	it("uses the selected folder name as its project label", () => {
		expect(repositoryNameFromWorkspaceRoot("/projects/New App/")).toBe("New App");
	});
});

describe("last task repository", () => {
	const repositories = [
		{ id: "alpha", rootPath: "/projects/alpha" },
		{ id: "beta", rootPath: "/projects/beta" },
	];

	it("prefers the remembered repository over the first repository", () => {
		expect(initialTaskRepository(repositories, null, "/projects/beta/")).toBe(
			repositories[1],
		);
	});

	it("lets an explicit project context override the remembered repository", () => {
		expect(
			initialTaskRepository(
				repositories,
				"/projects/alpha",
				"/projects/beta",
			),
		).toBe(repositories[0]);
	});

	it("falls back to the first available repository when the remembered one was removed", () => {
		expect(
			initialTaskRepository(repositories, null, "/projects/removed"),
		).toBe(repositories[0]);
	});

	it("persists a normalized path and tolerates unavailable storage", () => {
		const values = new Map<string, string>();
		const storage = {
			getItem: (key: string) => values.get(key) ?? null,
			setItem: (key: string, value: string) => values.set(key, value),
		};

		rememberLastTaskRepositoryRoot("/projects/beta/", storage);
		expect(values.get(LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY)).toBe(
			"/projects/beta",
		);
		expect(readLastTaskRepositoryRoot(storage)).toBe("/projects/beta");

		const unavailableStorage = {
			getItem: () => {
				throw new Error("unavailable");
			},
			setItem: () => {
				throw new Error("unavailable");
			},
		};
		expect(readLastTaskRepositoryRoot(unavailableStorage)).toBeNull();
		expect(() =>
			rememberLastTaskRepositoryRoot("/projects/alpha", unavailableStorage),
		).not.toThrow();
	});
});

describe("branch workspace entry", () => {
	it("opens directly in branch mode when launched from a project", () => {
		expect(initialWorkspaceStart(true)).toBe("branch");
		expect(initialWorkspaceStart(false)).toBe("new");
	});

	it("keeps pull requests in the dedicated Pull Requests surface", () => {
		expect(isBranchWorkspaceSource("branch")).toBe(true);
		expect(isBranchWorkspaceSource("pull_request")).toBe(false);
	});
});

describe("multi-project primary", () => {
	const repositories = [
		{ id: "dcc", name: "DevCommandCenter" },
		{ id: "lp", name: "DCC LP" },
		{ id: "api", name: "API" },
	];

	it("follows the order the person picked, not the project list", () => {
		expect(
			orderMultiProjectSelection(repositories, ["lp", "dcc"]).map(
				(repository) => repository.id,
			),
		).toEqual(["lp", "dcc"]);
	});

	it("drops ids that are no longer projects", () => {
		expect(
			orderMultiProjectSelection(repositories, ["gone", "api"]).map(
				(repository) => repository.id,
			),
		).toEqual(["api"]);
	});

	it("moves the chosen project to the front and keeps the rest in order", () => {
		expect(promoteMultiProjectPrimary(["dcc", "lp", "api"], "api")).toEqual([
			"api",
			"dcc",
			"lp",
		]);
	});

	it("ignores a project that is not selected", () => {
		expect(promoteMultiProjectPrimary(["dcc", "lp"], "api")).toEqual(["dcc", "lp"]);
	});
});

describe("linked folders in a multi-project task", () => {
	const projects = [
		{ id: "dcc", rootPath: "/code/dcc" },
		{ id: "lp", rootPath: "/code/lp" },
	];
	const linked = { dcc: ["/code/lp", "/code/docs-site/"] };
	const folder = (path: string) => ({ id: linkedFolderOptionId(path), rootPath: path });

	it("adds the linked folders that are not projects yet, once", () => {
		const options = withLinkedFolderOptions(projects, { ...linked, lp: ["/code/docs-site"] }, folder);
		expect(options.map((option) => option.id)).toEqual(["dcc", "lp", "linked:/code/docs-site"]);
	});

	it("brings the primary's linked folders along on the first pick", () => {
		const options = withLinkedFolderOptions(projects, linked, folder);
		expect(selectMultiProject([], "dcc", options, linked)).toEqual([
			"dcc",
			"lp",
			"linked:/code/docs-site",
		]);
	});

	it("adds only the picked project once a primary exists", () => {
		const options = withLinkedFolderOptions(projects, linked, folder);
		expect(selectMultiProject(["lp"], "dcc", options, linked)).toEqual(["lp", "dcc"]);
		expect(selectMultiProject(["lp"], "lp", options, linked)).toEqual(["lp"]);
	});
});
