import { describe, expect, it } from "vitest";
import type { SkillContextDetection } from "@/lib/skills-api";
import { getTotalSkillContextCount } from "./skill-count";

function detection(
	kind: SkillContextDetection["kind"],
	count: number,
	externalCount: number,
): SkillContextDetection {
	return {
		id: kind,
		kind,
		title: kind,
		relativePath: ".",
		rootKind: "target_root",
		count,
		managedCount: count - externalCount,
		externalCount,
		hasDccBlock: false,
	};
}

describe("getTotalSkillContextCount", () => {
	it("counts DCC source skills plus external native skills", () => {
		expect(
			getTotalSkillContextCount([
				detection("dcc_source", 3, 0),
				detection("claude_skills", 3, 0),
				detection("codex_skills", 2, 2),
				detection("cursor_rules", 4, 1),
			]),
		).toBe(6);
	});

	it("does not double-count DCC-managed compiled targets", () => {
		expect(
			getTotalSkillContextCount([
				detection("dcc_source", 2, 0),
				detection("claude_skills", 2, 0),
				detection("cursor_rules", 2, 0),
				detection("instructions_file", 1, 0),
			]),
		).toBe(2);
	});
});
