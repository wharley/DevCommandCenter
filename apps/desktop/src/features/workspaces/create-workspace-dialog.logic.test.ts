import { describe, expect, it } from "vitest";
import {
	includePickedRepository,
	inferProjectIdFromWorkspaceRoot,
	initialTaskRepository,
	initialWorkspaceStart,
	isBranchWorkspaceSource,
	LAST_TASK_REPOSITORY_ROOT_STORAGE_KEY,
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
