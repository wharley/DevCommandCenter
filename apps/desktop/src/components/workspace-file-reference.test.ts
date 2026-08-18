import { describe, expect, it } from "vitest";
import { parseWorkspaceFileReference } from "./workspace-file-reference";

describe("parseWorkspaceFileReference", () => {
	const workspaceRoot = "/Users/dev/project";

	it("parses relative paths with line and column", () => {
		expect(
			parseWorkspaceFileReference("apps/desktop/src/App.tsx:42:8", workspaceRoot),
		).toEqual({
			path: "apps/desktop/src/App.tsx",
			line: 42,
			column: 8,
		});
	});

	it("parses absolute file links inside the workspace", () => {
		expect(
			parseWorkspaceFileReference(
				"file:///Users/dev/project/src/main.rs%3A17",
				workspaceRoot,
			),
		).toEqual({ path: "src/main.rs", line: 17, column: null });
	});

	it("parses GitHub-style line fragments", () => {
		expect(
			parseWorkspaceFileReference("./src/main.ts#L9C3", workspaceRoot),
		).toEqual({ path: "src/main.ts", line: 9, column: 3 });
	});

	it("supports Windows workspace paths", () => {
		expect(
			parseWorkspaceFileReference(
				"C:\\work\\dcc\\src\\main.ts:11:2",
				"C:\\work\\dcc",
			),
		).toEqual({ path: "src/main.ts", line: 11, column: 2 });
	});

	it("rejects external URLs, traversal and paths outside the workspace", () => {
		expect(
			parseWorkspaceFileReference("https://example.com/file.ts", workspaceRoot),
		).toBeNull();
		expect(parseWorkspaceFileReference("../secret.txt", workspaceRoot)).toBeNull();
		expect(
			parseWorkspaceFileReference("/Users/dev/project-other/file.ts", workspaceRoot),
		).toBeNull();
	});
});
