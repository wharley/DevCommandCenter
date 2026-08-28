import { describe, expect, it } from "vitest";
import { parseWorkspacePatch } from "./workspace-patch-diff.logic";

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
