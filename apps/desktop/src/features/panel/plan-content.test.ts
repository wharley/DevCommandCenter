import { describe, expect, it } from "vitest";
import {
	buildCollapsedPlanPreviewMarkdown,
	buildPlanImplementationPrompt,
	buildPlanImplementationThreadTitle,
	normalizePlanContentForExport,
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
});
