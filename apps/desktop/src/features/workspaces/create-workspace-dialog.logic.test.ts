import { describe, expect, it } from "vitest";
import { inferProjectIdFromWorkspaceRoot } from "./create-workspace-dialog.logic";

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
