import { describe, expect, it } from "vitest";
import {
	detectIndentation,
	lineBreakInsertion,
	matchesEditorConfigGlob,
	resolveFileIndentation,
} from "./indentation";

const editorConfig = `
root = true

[*]
indent_style = space
indent_size = 2

[*.{rs,py}]
indent_size = 4

[Makefile]
indent_style = tab

[apps/legacy/**]
indent_style = tab
tab_width = 8
`;

describe("matchesEditorConfigGlob", () => {
	it("matches names at any depth and anchors globs with a slash", () => {
		expect(matchesEditorConfigGlob("*.{rs,py}", "crates/core/src/lib.rs")).toBe(true);
		expect(matchesEditorConfigGlob("*.{rs,py}", "src/main.ts")).toBe(false);
		expect(matchesEditorConfigGlob("Makefile", "tools/Makefile")).toBe(true);
		expect(matchesEditorConfigGlob("apps/legacy/**", "apps/legacy/a/b.js")).toBe(true);
		expect(matchesEditorConfigGlob("apps/legacy/**", "x/apps/legacy/b.js")).toBe(false);
	});
});

describe("resolveFileIndentation", () => {
	it("follows the last matching .editorconfig section", () => {
		expect(resolveFileIndentation(editorConfig, "src/App.tsx", "")).toEqual({ useTabs: false, size: 2 });
		expect(resolveFileIndentation(editorConfig, "crates/lib.rs", "")).toEqual({ useTabs: false, size: 4 });
		expect(resolveFileIndentation(editorConfig, "Makefile", "")).toEqual({ useTabs: true, size: 2 });
		expect(resolveFileIndentation(editorConfig, "apps/legacy/old.js", "")).toEqual({ useTabs: true, size: 8 });
	});

	it("falls back to what the file does", () => {
		expect(resolveFileIndentation(null, "a.ts", "if (a) {\n\treturn 1;\n}\n")).toEqual({ useTabs: true, size: 4 });
		expect(
			resolveFileIndentation(null, "a.py", "def f():\n    if x:\n        return 1\n"),
		).toEqual({ useTabs: false, size: 4 });
		expect(resolveFileIndentation(null, "a.txt", "plain\n")).toEqual({ useTabs: false, size: 2 });
	});
});

describe("detectIndentation", () => {
	it("does not count block comment stars as indentation", () => {
		expect(detectIndentation("/**\n * doc\n */\nconst a = 1;\n")).toEqual({});
	});
});

describe("lineBreakInsertion", () => {
	it("keeps the line's indentation", () => {
		expect(lineBreakInsertion("\t\tconst a = 1;", 15, "\t", "\n")).toEqual({
			text: "\n\t\t",
			caretLineOffset: 1,
			caretCharacter: 2,
		});
	});

	it("goes one level in after an opening bracket", () => {
		expect(lineBreakInsertion("  if (a) {", 10, "  ", "\n")).toEqual({
			text: "\n    ",
			caretLineOffset: 1,
			caretCharacter: 4,
		});
	});

	it("moves a closing bracket right after the caret to its own line", () => {
		expect(lineBreakInsertion("  call({})", 8, "  ", "\n")).toEqual({
			text: "\n    \n  ",
			caretLineOffset: 1,
			caretCharacter: 4,
		});
	});
});
