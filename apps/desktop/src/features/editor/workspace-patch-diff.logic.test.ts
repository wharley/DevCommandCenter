import { describe, expect, it } from "vitest";
import { parseWorkspacePatch, patchSelectionRequests } from "./workspace-patch-diff.logic";

const JSON_PATCH = `diff --git a/messages/en.json b/messages/en.json
index c8392b9..f634d05 100644
--- a/messages/en.json
+++ b/messages/en.json
@@ -12,1 +12,1 @@
-    "version": "v0.1.50"
+    "version": "v0.1.57"
`;

describe("parseWorkspacePatch", () => {
	it("parses the captured single-file Git patch", () => {
		const parsed = parseWorkspacePatch(JSON_PATCH);
		expect(parsed.name).toBe("messages/en.json");
		expect(parsed.hunks).toHaveLength(1);
		expect(parsed.additionLines).toHaveLength(1);
		expect(parsed.deletionLines).toHaveLength(1);
	});

	it("rejects a multi-file payload without exposing its contents", () => {
		const secondFile = JSON_PATCH.replaceAll("messages/en.json", "secret.txt");
		expect(() => parseWorkspacePatch(`${JSON_PATCH}${secondFile}`)).toThrow(
			"Captured turn patch must contain exactly one file diff",
		);
		try {
			parseWorkspacePatch(`${JSON_PATCH}${secondFile}`);
		} catch (error) {
			expect(String(error)).not.toContain("secret.txt");
		}
	});
});

describe("captured patch selections", () => {
	const patch = parseWorkspacePatch(`diff --git a/example.ts b/example.ts
--- a/example.ts
+++ b/example.ts
@@ -12,3 +12,4 @@
 before
-old
+new
+extra
 after
@@ -80,1 +81,1 @@
-later old
+later new
`);

	it("extracts a reversed multi-line selection at source offsets", () => {
		expect(patchSelectionRequests("example.ts", patch, { start: 15, end: 13, side: "additions" })).toEqual([
			{ path: "example.ts", side: "modified", startLine: 13, endLine: 15, snippet: "new\nextra\nafter" },
		]);
	});
	it("keeps removed and added code when selection crosses sides", () => {
		expect(patchSelectionRequests("example.ts", patch, { start: 13, end: 14, side: "deletions", endSide: "additions" })).toEqual([
			{ path: "example.ts", side: "original", startLine: 13, endLine: 13, snippet: "old" },
			{ path: "example.ts", side: "modified", startLine: 13, endLine: 14, snippet: "new\nextra" },
		]);
	});
	it("reads a later hunk without inventing omitted lines", () => {
		const requests = patchSelectionRequests("example.ts", patch, { start: 15, end: 81, side: "additions" });
		expect(requests).toEqual([
			{ path: "example.ts", side: "modified", startLine: 15, endLine: 15, snippet: "after" },
			{ path: "example.ts", side: "original", startLine: 80, endLine: 80, snippet: "later old" },
			{ path: "example.ts", side: "modified", startLine: 81, endLine: 81, snippet: "later new" },
		]);
	});
	it("supports a single removed line and rejects unavailable endpoints", () => {
		expect(patchSelectionRequests("example.ts", patch, { start: 80, end: 80, side: "deletions" })[0]?.snippet).toBe("later old");
		expect(patchSelectionRequests("example.ts", patch, { start: 1, end: 2 })).toEqual([]);
	});
});
