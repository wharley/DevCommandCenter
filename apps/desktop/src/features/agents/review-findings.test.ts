import { describe, expect, it } from "vitest";
import {
	parseReviewFindings,
	reviewAnnotationsForPath,
	stripReviewBlock,
} from "./review-findings";

const block = (json: string) => `Found two problems.\n\n\`\`\`dcc-review\n${json}\n\`\`\``;

describe("review findings", () => {
	it("reads the reviewer's block and normalizes each finding", () => {
		const findings = parseReviewFindings(
			block(
				JSON.stringify({
					findings: [
						{
							path: "./src/a.ts",
							line: 10,
							endLine: 12,
							severity: "major",
							title: " Off by one ",
							detail: " The loop skips the last item. ",
						},
						{ path: "src/b.ts", line: 3, endLine: 1, severity: "blocker", title: "Null deref" },
					],
				}),
			),
		);
		expect(findings).toEqual([
			{
				path: "src/a.ts",
				startLine: 10,
				endLine: 12,
				severity: "major",
				title: "Off by one",
				detail: "The loop skips the last item.",
			},
			{ path: "src/b.ts", startLine: 3, endLine: 3, severity: "minor", title: "Null deref", detail: "" },
		]);
	});

	it("drops entries that cannot be placed on a line", () => {
		const findings = parseReviewFindings(
			block(
				JSON.stringify({
					findings: [
						{ path: "src/a.ts", line: 0, title: "zero" },
						{ path: "src/a.ts", line: 1.5, title: "fraction" },
						{ path: "", line: 1, title: "no path" },
						{ path: "src/a.ts", line: 1, title: " " },
						"text",
						{ path: "src/a.ts", line: 4, title: "kept" },
					],
				}),
			),
		);
		expect(findings?.map((finding) => finding.title)).toEqual(["kept"]);
	});

	it("tells a clean review apart from a review without a block", () => {
		expect(parseReviewFindings(block('{"findings":[]}'))).toEqual([]);
		expect(parseReviewFindings("No issues found.")).toBeNull();
		expect(parseReviewFindings(block("{not json"))).toBeNull();
		expect(parseReviewFindings(block('{"findings":"none"}'))).toBeNull();
	});

	it("survives a code fence quoted inside a finding", () => {
		const message = block(
			JSON.stringify({
				findings: [
					{ path: "src/a.ts", line: 2, title: "Guard missing", detail: "Add ```if (!b) throw``` first." },
				],
			}),
		);
		expect(parseReviewFindings(message)?.[0]?.detail).toBe("Add ```if (!b) throw``` first.");
		expect(stripReviewBlock(message)).toBe("Found two problems.");
	});

	it("hides the block from the message, even while it is still streaming", () => {
		expect(stripReviewBlock(block('{"findings":[]}'))).toBe("Found two problems.");
		expect(stripReviewBlock('Found one.\n\n```dcc-review\n{"findings":[{"pa')).toBe("Found one.");
		expect(stripReviewBlock("Plain answer with ```ts\ncode\n```")).toBe(
			"Plain answer with ```ts\ncode\n```",
		);
	});

	it("marks only the findings of the open file, on the new side", () => {
		const findings = parseReviewFindings(
			block(
				JSON.stringify({
					findings: [
						{ path: "src/a.ts", line: 10, severity: "critical", title: "A" },
						{ path: "src/b.ts", line: 3, title: "B" },
					],
				}),
			),
		);
		expect(reviewAnnotationsForPath("src/a.ts", findings ?? [])).toEqual([
			{
				source: "agent-review",
				severity: "critical",
				side: "modified",
				startLine: 10,
				endLine: 10,
				title: "A",
				detail: "",
			},
		]);
	});
});
