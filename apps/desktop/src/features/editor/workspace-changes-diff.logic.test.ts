import { describe, expect, it } from "vitest";
import {
	annotationPayloadFromPierreRange,
	groupWorkspaceDiffAnnotations,
	workspaceDiffAnnotationCss,
	workspaceDiffContentHash,
} from "./workspace-changes-diff.logic";

describe("workspace changes diff logic", () => {
	it("maps a reversed addition selection to the modified snippet", () => {
		expect(
			annotationPayloadFromPierreRange({
				range: { start: 3, end: 2, side: "additions" },
				originalText: "old one\nold two\nold three",
				modifiedText: "new one\nnew two\nnew three",
				anchor: { top: 10, left: 20 },
			}),
		).toEqual({
			side: "modified",
			startLine: 2,
			endLine: 3,
			snippet: "new two\nnew three",
			anchor: { top: 10, left: 20 },
		});
	});

	it("maps deletion-side selections to original lines", () => {
		const payload = annotationPayloadFromPierreRange({
			range: { start: 1, end: 2, side: "deletions" },
			originalText: "old one\nold two\nold three",
			modifiedText: "new one\nnew two",
			anchor: { top: 0, left: 0 },
		});
		expect(payload.side).toBe("original");
		expect(payload.snippet).toBe("old one\nold two");
	});

	it("anchors cross-side selections to the ending side", () => {
		const payload = annotationPayloadFromPierreRange({
			range: {
				start: 2,
				end: 4,
				side: "deletions",
				endSide: "additions",
			},
			originalText: "a\nb\nc\nd",
			modifiedText: "A\nB\nC\nD",
			anchor: { top: 0, left: 0 },
		});
		expect(payload).toMatchObject({
			side: "modified",
			startLine: 4,
			endLine: 4,
			snippet: "D",
		});
	});

	it("groups callouts without overwriting findings on the same line", () => {
		const groups = groupWorkspaceDiffAnnotations([
			{
				source: "coderabbit",
				severity: "major",
				side: "modified",
				startLine: 2,
				endLine: 4,
				title: "First",
			},
			{
				source: "forge-review",
				severity: "info",
				side: "modified",
				startLine: 4,
				endLine: 4,
				title: "Second",
			},
		]);
		expect(groups).toHaveLength(1);
		expect(groups[0]?.annotations.map((entry) => entry.title)).toEqual([
			"First",
			"Second",
		]);
	});

	it("creates range styles for both diff sides and caps malformed ranges", () => {
		const css = workspaceDiffAnnotationCss([
			{
				source: "coderabbit",
				severity: "major",
				side: "original",
				startLine: 2,
				endLine: 3,
				title: "Old",
			},
			{
				source: "forge-review",
				severity: "info",
				side: "modified",
				startLine: 8,
				endLine: 10_000,
				title: "New",
			},
		]);
		expect(css).toContain('[data-deletions] [data-line="2"]');
		expect(css).toContain('[data-deletions] [data-line="3"]');
		expect(css).toContain('[data-additions] [data-line="8"]');
		expect(css).toContain('[data-additions] [data-line="507"]');
		expect(css).not.toContain('[data-additions] [data-line="508"]');
	});

	it("hashes content deterministically and distinguishes same-length revisions", () => {
		expect(workspaceDiffContentHash("alpha")).toBe(
			workspaceDiffContentHash("alpha"),
		);
		expect(workspaceDiffContentHash("alpha")).not.toBe(
			workspaceDiffContentHash("omega"),
		);
	});
});
