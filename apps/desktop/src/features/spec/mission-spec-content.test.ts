import { describe, expect, it } from "vitest";
import {
	buildMissionAcceptanceCriteriaCoverage,
	buildMissionContinueCriterionPrompt,
	buildMissionReanchorPrompt,
	buildMissionResumeContext,
	buildMissionValidationPrompt,
	buildMissionValidationSavePayload,
	computeMissionSpecHash,
	parseMissionValidationReport,
	parseMissionAcceptanceCriteria,
	parseMissionValidationChecks,
	parseMissionSuggestedValidationChecks,
	parseMissionValidationPersistence,
	parseMissionValidationProfiles,
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

	it("parses validation checks from frontmatter and markdown", () => {
		const spec = [
			"---",
			"status: draft",
			"validation_checks:",
			"  - Run `yarn workspace @dcc/desktop typecheck`.",
			"---",
			"# Spec",
			"",
			"## Validation Checks",
			"- Run `yarn workspace @dcc/desktop test`.",
			"- Run `yarn workspace @dcc/desktop typecheck`.",
		].join("\n");

		expect(parseMissionValidationChecks(spec)).toEqual([
			{ text: "Run `yarn workspace @dcc/desktop typecheck`." },
			{ text: "Run `yarn workspace @dcc/desktop test`." },
		]);
	});

	it("parses validation profiles and persistence from frontmatter", () => {
		const spec = [
			"---",
			"validation_profile: tauri",
			"validation_profiles:",
			"  - rust",
			"  - react",
			"validation_persistence: auto",
			"---",
			"# Spec",
		].join("\n");

		expect(parseMissionValidationProfiles(spec)).toEqual([
			"rust",
			"react",
			"tauri",
		]);
		expect(parseMissionValidationPersistence(spec)).toBe("auto");
	});

	it("builds suggested validation checks from declared profiles", () => {
		const spec = [
			"---",
			"validation_profiles:",
			"  - rust",
			"  - tauri",
			"---",
			"# Spec",
		].join("\n");

		expect(parseMissionSuggestedValidationChecks(spec)).toEqual([
			{
				text: "Run the relevant cargo check command for the affected crate or workspace.",
			},
			{
				text: "Run the relevant cargo test command when tests exist for the affected crate or workspace.",
			},
			{
				text: "Run the desktop frontend check and the Rust backend check used by the repository.",
			},
			{
				text: "Exercise the affected desktop flow manually before concluding PASS.",
			},
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
		const specMarkdown = "## Acceptance Criteria\n- AC-1: Visible spec.";
		const prompt = buildMissionValidationPrompt({
			specRelativePath: ".devcommandcenter/specs/demo.spec.md",
			specMarkdown,
			planMarkdown: "- [x] Add Spec tab. Covers AC-1.",
		});

		expect(prompt).toContain("VALIDATE THIS MISSION AGAINST ITS SPEC.");
		expect(prompt).toContain("Do not modify files.");
		expect(prompt).toContain("PASS, FAIL, or UNKNOWN");
		expect(prompt).toContain("RUN, SKIPPED, or BLOCKED");
		expect(prompt).toContain("dccMissionValidation");
		expect(prompt).toContain(computeMissionSpecHash(specMarkdown));
		expect(prompt).toContain(".devcommandcenter/specs/demo.spec.md");
		expect(prompt).toContain("CURRENT PLAN CONTEXT:");
		expect(prompt).toContain("AC-1");
	});

	it("includes spec-defined validation checks in the validation prompt", () => {
		const specMarkdown = [
			"## Validation Checks",
			"- Run `yarn workspace @dcc/desktop typecheck`.",
			"",
			"## Acceptance Criteria",
			"- AC-1: Visible spec.",
		].join("\n");
		const prompt = buildMissionValidationPrompt({
			specMarkdown,
		});

		expect(prompt).toContain(
			"Prioritize these validation checks declared by the spec before concluding PASS:",
		);
		expect(prompt).toContain(
			"Include one checks[] row for every declared or suggested validation check.",
		);
		expect(prompt).toContain("Run `yarn workspace @dcc/desktop typecheck`.");
	});

	it("falls back to profile-based validation checks when explicit checks are absent", () => {
		const specMarkdown = [
			"---",
			"validation_profiles:",
			"  - rust",
			"---",
			"",
			"## Acceptance Criteria",
			"- AC-1: Visible spec.",
		].join("\n");
		const prompt = buildMissionValidationPrompt({
			specMarkdown,
		});

		expect(prompt).toContain(
			"No explicit validation checks were declared. Start from these defaults suggested by the spec validation profiles:",
		);
		expect(prompt).toContain(
			"Run the relevant cargo check command for the affected crate or workspace.",
		);
	});

	it("builds a save payload with validation provenance metadata", () => {
		const payload = buildMissionValidationSavePayload({
			rawJson: JSON.stringify({
				dccMissionValidation: true,
				specRelativePath: ".devcommandcenter/specs/demo.spec.md",
				specHash: "fnv1a32:12345678",
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Visible.",
						nextAction: "",
					},
				],
			}),
			mode: "auto",
			savedAt: "2026-05-23T12:34:56.000Z",
		});

		expect(payload).not.toBeNull();
		expect(JSON.parse(payload ?? "{}")).toMatchObject({
			dccPersistenceMode: "auto",
			dccSavedAt: "2026-05-23T12:34:56.000Z",
		});
	});

	it("builds a re-anchor prompt with spec, plan, and saved validation", () => {
		const prompt = buildMissionReanchorPrompt({
			specMarkdown: "# Spec\n\n## Acceptance Criteria\n- AC-1: Visible spec.",
			planMarkdown: "- [ ] Cover AC-1.",
			validationJson: '{"dccMissionValidation":true}',
		});

		expect(prompt).toContain("RE-ANCHOR THIS SESSION");
		expect(prompt).toContain("Do not implement yet.");
		expect(prompt).toContain("MISSION SPEC:");
		expect(prompt).toContain("ACTIVE PLAN:");
		expect(prompt).toContain("SAVED VALIDATION VERDICT:");
	});

	it("adds pending acceptance criteria to the re-anchor prompt", () => {
		const specMarkdown = [
			"# Spec",
			"",
			"## Acceptance Criteria",
			"- AC-1: Visible spec.",
			"- AC-2: Persist validation.",
		].join("\n");
		const prompt = buildMissionReanchorPrompt({
			specMarkdown,
			validationJson: JSON.stringify({
				dccMissionValidation: true,
				specHash: computeMissionSpecHash(specMarkdown),
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Spec tab rendered.",
						nextAction: "",
					},
					{
						id: "AC-2",
						status: "FAIL",
						evidence: "No saved file found.",
						nextAction: "Save validation verdict.",
					},
				],
			}),
		});

		expect(prompt).toContain("RESUME CONTEXT:");
		expect(prompt).toContain(
			"- AC-2 [FAIL]: Persist validation. Next: Save validation verdict.",
		);
		expect(prompt).not.toContain("- AC-1 [PASS]");
	});

	it("builds structured resume context for the inspector", () => {
		const specMarkdown = [
			"# Spec",
			"",
			"## Acceptance Criteria",
			"- AC-1: Visible spec.",
			"- AC-2: Persist validation.",
			"- AC-3: Re-anchor after compact.",
		].join("\n");

		expect(
			buildMissionResumeContext({
				specMarkdown,
				validationJson: JSON.stringify({
					dccMissionValidation: true,
					specHash: computeMissionSpecHash(specMarkdown),
					criteria: [
						{ id: "AC-1", status: "PASS", evidence: "Done.", nextAction: "" },
						{
							id: "AC-2",
							status: "UNKNOWN",
							evidence: "No saved verdict checked.",
							nextAction: "Inspect saved validation.",
						},
					],
				}),
			}),
		).toMatchObject({
			state: "pending",
			criteria: [
				{
					id: "AC-2",
					status: "UNKNOWN",
					description: "Persist validation.",
					nextAction: "Inspect saved validation.",
				},
				{
					id: "AC-3",
					status: "UNKNOWN",
					description: "Re-anchor after compact.",
					nextAction: "Validate this acceptance criterion.",
				},
			],
		});
	});

	it("builds a prompt to continue the next pending criterion", () => {
		const prompt = buildMissionContinueCriterionPrompt({
			specMarkdown: "## Acceptance Criteria\n- AC-2: Persist validation.",
			planMarkdown: "- [ ] Save verdict flow. Covers AC-2.",
			validationJson: '{"dccMissionValidation":true}',
			criterion: {
				id: "AC-2",
				description: "Persist validation.",
				status: "FAIL",
				evidence: "No saved file found.",
				nextAction: "Save validation verdict.",
			},
		});

		expect(prompt).toContain("CONTINUE THE NEXT PENDING MISSION CRITERION.");
		expect(prompt).toContain("TARGET CRITERION: AC-2 [FAIL]");
		expect(prompt).toContain("Suggested next action: Save validation verdict.");
		expect(prompt).toContain("ACTIVE PLAN:");
		expect(prompt).toContain("SAVED VALIDATION VERDICT:");
	});

	it("marks stale validation as historical context in re-anchor prompts", () => {
		const prompt = buildMissionReanchorPrompt({
			specMarkdown: "## Acceptance Criteria\n- AC-1: Visible spec changed.",
			validationJson: JSON.stringify({
				dccMissionValidation: true,
				specHash: "fnv1a32:00000000",
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Old evidence.",
						nextAction: "",
					},
				],
			}),
		});

		expect(prompt).toContain("Saved validation is stale");
		expect(prompt).toContain("- AC-1 [UNKNOWN]: Visible spec changed.");
	});

	it("treats validation without a spec hash as historical context", () => {
		const prompt = buildMissionReanchorPrompt({
			specMarkdown: "## Acceptance Criteria\n- AC-1: Visible spec.",
			validationJson: JSON.stringify({
				dccMissionValidation: true,
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Old evidence.",
						nextAction: "",
					},
				],
			}),
		});

		expect(prompt).toContain("Saved validation has no spec hash");
		expect(prompt).toContain("- AC-1 [UNKNOWN]: Visible spec.");
	});

	it("parses a fenced structured validation report", () => {
		const report = parseMissionValidationReport([
			"Validation complete.",
			"",
			"```json",
			JSON.stringify({
				dccMissionValidation: true,
				specRelativePath: ".devcommandcenter/specs/demo.spec.md",
				specHash: "fnv1a32:12345678",
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
				checks: [
					{
						text: "Run typecheck",
						status: "RUN",
						evidence: "yarn workspace @dcc/desktop typecheck",
					},
				],
			}),
			"```",
		].join("\n"));

		expect(report).toEqual({
			specRelativePath: ".devcommandcenter/specs/demo.spec.md",
			specHash: "fnv1a32:12345678",
			summary: "One pass, one unknown.",
			rawJson: expect.stringContaining("dccMissionValidation"),
			checks: [
				{
					text: "Run typecheck",
					status: "RUN",
					evidence: "yarn workspace @dcc/desktop typecheck",
				},
			],
			persistenceMode: null,
			persistedAt: null,
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

	it("parses saved validation provenance metadata", () => {
		const report = parseMissionValidationReport(
			JSON.stringify({
				dccMissionValidation: true,
				specRelativePath: ".devcommandcenter/specs/demo.spec.md",
				specHash: "fnv1a32:12345678",
				dccPersistenceMode: "manual",
				dccSavedAt: "2026-05-23T12:34:56.000Z",
				criteria: [
					{
						id: "AC-1",
						status: "PASS",
						evidence: "Saved.",
						nextAction: "",
					},
				],
			}),
		);

		expect(report).toMatchObject({
			persistenceMode: "manual",
			persistedAt: "2026-05-23T12:34:56.000Z",
		});
	});
});
