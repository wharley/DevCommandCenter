import { describe, expect, it } from "vitest";
import { diffBlocks, parsePatch } from "./patch.ts";

const PATCH = `diff --git a/app.json b/app.json
index 1..2 100644
--- a/app.json
+++ b/app.json
@@ -8,3 +8,3 @@ "expo": {
     "icon": "./assets/icon.png",
-    "userInterfaceStyle": "dark",
+    "userInterfaceStyle": "automatic",
     "backgroundColor": "#0a0b0d",
\\ No newline at end of file`;

describe("parsePatch", () => {
	it("drops file headers and numbers each line", () => {
		const lines = parsePatch(PATCH);
		expect(lines[0]).toEqual({ kind: "hunk", text: '@@ -8,3 +8,3 @@ "expo": {' });
		expect(lines.slice(1, 5)).toEqual([
			{ kind: "context", text: '    "icon": "./assets/icon.png",', oldLine: 8, newLine: 8 },
			{ kind: "del", text: '    "userInterfaceStyle": "dark",', oldLine: 9, newLine: null },
			{ kind: "add", text: '    "userInterfaceStyle": "automatic",', oldLine: null, newLine: 9 },
			{ kind: "context", text: '    "backgroundColor": "#0a0b0d",', oldLine: 10, newLine: 10 },
		]);
		expect(lines.at(-1)).toEqual({ kind: "note", text: "Sem quebra de linha no fim do arquivo" });
	});

	it("says when the file is binary", () => {
		expect(parsePatch("diff --git a/x b/x\nBinary files a/x and b/x differ")).toEqual([{ kind: "note", text: "Arquivo binário" }]);
	});
});

describe("diffBlocks", () => {
	it("keeps five blocks and shows both sides when both changed", () => {
		expect(diffBlocks(48, 12)).toEqual(["add", "add", "add", "add", "del"]);
		expect(diffBlocks(500, 1)).toEqual(["add", "add", "add", "add", "del"]);
		expect(diffBlocks(1, 500)).toEqual(["add", "del", "del", "del", "del"]);
		expect(diffBlocks(0, 0)).toEqual(["none", "none", "none", "none", "none"]);
		expect(diffBlocks(3, 0)).toEqual(["add", "add", "add", "add", "add"]);
	});
});
