import { describe, expect, it } from "vitest";
import {
	buildMissionAcceptanceCriteriaCoverage,
	buildMissionValidationPrompt,
	parseMissionValidationReport,
	parseMissionAcceptanceCriteria,
} from "./mission-spec-content";

describe("mission-spec-content", () => {
	it("parses acceptance criteria from frontmatter and markdown", () => {
		const spec = [
			"---",
			"status: draft",
			"acceptance_criteria:",
			"  - id: AC-1",
			"    description: Workspace spec is visible.",
			"---",
			"# Spec",
			"",
			"## Acceptance Criteria",
			"- AC-2: Plan can be generated from the spec.",
		].join("\n");

		expect(parseMissionAcceptanceCriteria(spec)).toEqual([
			{ id: "AC-1", description: "Workspace spec is visible." },
			{ id: "AC-2", description: "Plan can be generated from the spec." },
		]);
	});

	it("marks criteria as covered when the plan references their ids", () => {
		const coverage = buildMissionAcceptanceCriteriaCoverage(
			[
				{ id: "AC-1", description: "Visible spec." },
				{ id: "AC-2", description: "Generated plan." },
			],
			"- [ ] Add a Spec tab. Covers AC-1.",
		);

		expect(coverage).toEqual([
			{ id: "AC-1", description: "Visible spec.", covered: true },
			{ id: "AC-2", description: "Generated plan.", covered: false },
		]);
	});

	it("builds a validation prompt that asks for evidence without edits", () => {
		const prompt = buildMissionValidationPrompt({
			specMarkdown: "## Acceptance Criteria\n- AC-1: Visible spec.",
			planMarkdown: "- [x] Add Spec tab. Covers AC-1.",
		});

		expect(prompt).toContain("VALIDATE THIS MISSION AGAINST ITS SPEC.");
		expect(prompt).toContain("Do not modify files.");
		expect(prompt).toContain("PASS, FAIL, or UNKNOWN");
		expect(prompt).toContain("dccMissionValidation");
		expect(prompt).toContain("CURRENT PLAN CONTEXT:");
		expect(prompt).toContain("AC-1");
	});

	it("parses a fenced structured validation report", () => {
		const report = parseMissionValidationReport([
			"Validation complete.",
			"",
			"```json",
			JSON.stringify({
				dccMissionValidation: true,
				summary: "One pass, one unknown.",
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Spec tab renders.",
						nextAction: "",
					},
					{
						id: "AC-2",
						status: "UNKNOWN",
						evidence: "No test evidence found.",
						nextAction: "Run the UI flow.",
					},
				],
			}),
			"```",
		].join("\n"));

		expect(report).toEqual({
			summary: "One pass, one unknown.",
			rawJson: expect.stringContaining("dccMissionValidation"),
			criteria: [
				{
					id: "AC-1",
					status: "PASS",
					evidence: "Spec tab renders.",
					nextAction: "",
				},
				{
					id: "AC-2",
					status: "UNKNOWN",
					evidence: "No test evidence found.",
					nextAction: "Run the UI flow.",
				},
			],
		});
	});
});
