import { describe, expect, it } from "vitest";
import { parseUnifiedDiff } from "./unified-diff";

describe("parseUnifiedDiff", () => {
	it("tracks old and new line coordinates across a hunk", () => {
		const rows = parseUnifiedDiff(
			"@@ -10,3 +10,4 @@\n context\n-old\n+new\n+extra\n unchanged",
		);

		expect(rows.map((row) => [row.kind, row.oldLine, row.newLine])).toEqual([
			["hunk", null, null],
			["context", 10, 10],
			["deletion", 11, null],
			["addition", null, 11],
			["addition", null, 12],
			["context", 12, 13],
		]);
		expect(rows[2].reviewSide).toBe("left");
		expect(rows[3].reviewSide).toBe("right");
	});

	it("returns metadata rows without review coordinates", () => {
		const rows = parseUnifiedDiff("Binary files differ");
		expect(rows).toEqual([
			expect.objectContaining({
				kind: "meta",
				reviewLine: null,
				reviewSide: null,
			}),
		]);
	});
});
