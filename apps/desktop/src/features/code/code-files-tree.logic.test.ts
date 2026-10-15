import { describe, expect, it } from "vitest";
import {
	ancestorFolderPaths,
	buildCodeTree,
	codeTreeGitStatus,
	fileSetSignature,
	visibleCodeTreeRows,
} from "./code-files-tree.logic";

const paths = [
	"README.md",
	"apps/desktop/src/App.tsx",
	"apps/desktop/src/main.tsx",
	"apps/desktop/package.json",
	"crates/core/src/lib.rs",
];

describe("visibleCodeTreeRows", () => {
	it("lists folders before files, everything collapsed", () => {
		const rows = visibleCodeTreeRows(buildCodeTree(paths), new Set());
		expect(rows.map((row) => [row.kind, row.label])).toEqual([
			["folder", "apps/desktop"],
			["folder", "crates/core/src"],
			["file", "README.md"],
		]);
	});

	it("keys a compacted chain on its deepest folder", () => {
		const rows = visibleCodeTreeRows(buildCodeTree(paths), new Set(["apps/desktop"]));
		expect(rows.map((row) => [row.label, row.depth, row.parentPath])).toEqual([
			["apps/desktop", 0, null],
			["src", 1, "apps/desktop"],
			["package.json", 1, "apps/desktop"],
			["crates/core/src", 0, null],
			["README.md", 0, null],
		]);
	});

	it("opens the way down to a revealed file", () => {
		const expanded = new Set(ancestorFolderPaths("apps/desktop/src/App.tsx"));
		const rows = visibleCodeTreeRows(buildCodeTree(paths), expanded);
		expect(rows.some((row) => row.path === "apps/desktop/src/App.tsx")).toBe(true);
	});
});

describe("codeTreeGitStatus", () => {
	it("prefers the worktree status and marks the folders above a change", () => {
		const { byPath, changedFolders } = codeTreeGitStatus(
			[{ path: "apps/desktop/src/App.tsx", status: "A" }],
			[
				{ path: "apps/desktop/src/App.tsx", status: "M" },
				{ path: "notes/new.md", status: "?" },
			],
		);
		expect(byPath.get("apps/desktop/src/App.tsx")).toEqual({ letter: "M", tone: "modified" });
		expect(byPath.get("notes/new.md")).toEqual({ letter: "U", tone: "added" });
		expect([...changedFolders].sort()).toEqual([
			"apps",
			"apps/desktop",
			"apps/desktop/src",
			"notes",
		]);
	});
});

describe("fileSetSignature", () => {
	it("ignores edits and moves when a file is created or deleted", () => {
		const base = fileSetSignature([], [{ path: "a.ts", status: "M" }]);
		expect(fileSetSignature([], [{ path: "a.ts", status: "M" }, { path: "b.ts", status: "M" }])).toBe(base);
		expect(fileSetSignature([], [{ path: "c.ts", status: "?" }])).not.toBe(base);
	});
});
