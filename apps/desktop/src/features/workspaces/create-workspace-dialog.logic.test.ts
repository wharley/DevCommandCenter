import { describe, expect, it } from "vitest";
import {
	includePickedRepository,
	inferProjectIdFromWorkspaceRoot,
	initialWorkspaceStart,
	isBranchWorkspaceSource,
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
