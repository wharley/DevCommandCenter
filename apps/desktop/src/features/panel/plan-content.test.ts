import { describe, expect, it } from "vitest";
import {
	buildCollapsedPlanPreviewMarkdown,
	buildPlanDelegationPrompt,
	buildPlanFromSpecPrompt,
	buildPlanImplementationPrompt,
	buildPlanImplementationThreadTitle,
	normalizePlanContentForExport,
	parsePlanContent,
	stripDisplayedPlanMarkdown,
} from "./plan-content";

describe("plan-content", () => {
	it("strips the display-only headings from plan markdown", () => {
		const markdown = [
			"# Mission Plan",
			"",
			"## Summary",
			"",
			"Deliver the UI update.",
			"",
			"## Steps",
			"- [x] Inspect the chat flow",
			"- [ ] Add the plan card",
		].join("\n");

		expect(stripDisplayedPlanMarkdown(markdown)).toContain("Deliver the UI update.");
		expect(stripDisplayedPlanMarkdown(markdown)).not.toContain("# Mission Plan");
		expect(stripDisplayedPlanMarkdown(markdown)).not.toContain("## Summary");
	});

	it("builds a collapsed preview and keeps the trailing ellipsis when content is long", () => {
		const markdown = [
			"# Mission Plan",
			"",
			"## Summary",
			"",
			"Deliver the UI update.",
			"",
			"## Steps",
			"- [x] Inspect the chat flow",
			"- [ ] Add the plan card",
			"- [ ] Run typecheck",
			"- [ ] Verify the sidebar",
		].join("\n");

		const preview = buildCollapsedPlanPreviewMarkdown(markdown, { maxLines: 2 });
		expect(preview).toContain("Deliver the UI update.");
		expect(preview).toContain("...");
		expect(preview).not.toContain("# Mission Plan");
	});

	it("normalizes exports with a trailing newline", () => {
		expect(normalizePlanContentForExport("Plan text")).toBe("Plan text\n");
	});

	it("builds the implementation prompt and thread title", () => {
		const markdown = "# Mission Plan\n\nShip the dashboard.";
		expect(buildPlanImplementationPrompt(markdown)).toBe(
			"PLEASE IMPLEMENT THIS PLAN:\n# Mission Plan\n\nShip the dashboard.",
		);
		expect(buildPlanImplementationThreadTitle(markdown)).toBe("Implement Mission Plan");
	});

	it("builds a delegation prompt containing the full plan and execution criteria", () => {
		const markdown = "# Mission Plan\n\nShip the dashboard.";
		const prompt = buildPlanDelegationPrompt(markdown);

		expect(prompt).toContain("Implement the plan below using the current workspace.");
		expect(prompt).toContain("# Mission Plan\n\nShip the dashboard.");
		expect(prompt).toContain("Execution criteria:");
		expect(prompt).toContain("changed files");
	});

	it("builds a planning prompt from a mission spec without requesting implementation", () => {
		const spec = [
			"---",
			"status: draft",
			"acceptance_criteria:",
			"  - id: AC-1",
			"    description: Dashboard loads the current workspace.",
			"---",
			"# Dashboard spec",
		].join("\n");

		const prompt = buildPlanFromSpecPrompt(spec);

		expect(prompt).toContain("PLEASE TURN THIS SPEC INTO AN IMPLEMENTATION PLAN.");
		expect(prompt).toContain("Do not implement yet.");
		expect(prompt).toContain("criteria[]");
		expect(prompt).toContain("AC-1");
		expect(prompt).toContain("# Dashboard spec");
	});

	it("parses structured step criteria from json plans", () => {
		const parsed = parsePlanContent(
			JSON.stringify({
				title: "Mission Plan",
				summary: "Ship the flow.",
				steps: [
					{
						text: "Add save verdict refresh.",
						status: "pending",
						criteria: ["AC-1", "ac-2"],
					},
				],
			}),
		);

		expect(parsed.source).toBe("json");
		expect(parsed.steps).toEqual([
			expect.objectContaining({
				text: "Add save verdict refresh.",
				criteria: ["AC-1", "AC-2"],
			}),
		]);
		expect(parsed.markdown).toContain("Covers AC-1, AC-2.");
	});

	it("extracts acceptance criteria ids from markdown step text", () => {
		const parsed = parsePlanContent([
			"# Mission Plan",
			"",
			"## Steps",
			"- [ ] Save verdict and cover AC-3.",
		].join("\n"));

		expect(parsed.steps).toEqual([
			expect.objectContaining({
				criteria: ["AC-3"],
			}),
		]);
	});

	it("rejects malformed structured plan steps", () => {
		const parsed = parsePlanContent(
			JSON.stringify({
				title: "Broken plan",
				steps: [
					{
						status: "pending",
						criteria: ["AC-1"],
					},
				],
			}),
		);

		expect(parsed.source).toBe("markdown");
		expect(parsed.isPlanLike).toBe(false);
		expect(parsed.steps).toEqual([]);
	});

	it("rejects malformed structured criteria arrays", () => {
		const parsed = parsePlanContent(
			JSON.stringify({
				title: "Broken plan",
				steps: [
					{
						text: "Do the work.",
						criteria: ["AC-1", 42],
					},
				],
			}),
		);

		expect(parsed.source).toBe("markdown");
		expect(parsed.steps).toEqual([]);
	});
});
