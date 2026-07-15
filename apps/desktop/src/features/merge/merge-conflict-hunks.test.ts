import { describe, expect, it } from "vitest";
import {
	applyMergeConflictResolution,
	hasMergeConflictMarkerFragments,
	parseMergeConflictHunks,
} from "./merge-conflict-hunks";

const SIMPLE = [
	"before",
	"<<<<<<< HEAD",
	"const current = true;",
	"=======",
	"const incoming = true;",
	">>>>>>> origin/main",
	"after",
	"",
].join("\n");

describe("merge conflict hunks", () => {
	it("parses labels, ranges and both sides", () => {
		const [hunk] = parseMergeConflictHunks(SIMPLE);
		expect(hunk).toMatchObject({
			startLine: 2,
			endLine: 6,
			currentLabel: "HEAD",
			incomingLabel: "origin/main",
			currentText: "const current = true;\n",
			incomingText: "const incoming = true;\n",
		});
	});

	it("accepts current, incoming or both deterministically", () => {
		const [hunk] = parseMergeConflictHunks(SIMPLE);
		expect(applyMergeConflictResolution(SIMPLE, hunk!, "current")).toBe(
			"before\nconst current = true;\nafter\n",
		);
		expect(applyMergeConflictResolution(SIMPLE, hunk!, "incoming")).toBe(
			"before\nconst incoming = true;\nafter\n",
		);
		expect(applyMergeConflictResolution(SIMPLE, hunk!, "both")).toBe(
			"before\nconst current = true;\nconst incoming = true;\nafter\n",
		);
	});

	it("supports diff3 base markers and CRLF", () => {
		const source = [
			"<<<<<<< feature",
			"current",
			"||||||| base",
			"ancestor",
			"=======",
			"incoming",
			">>>>>>> main",
			"",
		].join("\r\n");
		const [hunk] = parseMergeConflictHunks(source);
		expect(hunk?.baseText).toBe("ancestor\r\n");
		expect(applyMergeConflictResolution(source, hunk!, "both")).toBe(
			"current\r\nincoming\r\n",
		);
	});

	it("re-parses remaining blocks after resolving one", () => {
		const source = `${SIMPLE}${SIMPLE}`;
		const first = parseMergeConflictHunks(source)[0]!;
		const next = applyMergeConflictResolution(source, first, "current");
		expect(parseMergeConflictHunks(next)).toHaveLength(1);
	});

	it("detects malformed marker fragments for a final safety warning", () => {
		expect(hasMergeConflictMarkerFragments("<<<<<<< HEAD\nmissing separator\n")).toBe(true);
		expect(hasMergeConflictMarkerFragments("const value = '<<<<<<<';\n")).toBe(false);
	});
});
