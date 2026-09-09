import type { SkillRecord } from "@/lib/skills-api";
import { ORCHESTRATION_PRESET } from "./orchestration-preset";

/** Bundled templates. Adding one creates an editable project copy. */
export const DCC_SKILL_CATALOG: ReadonlyArray<{
	skill: SkillRecord;
	titleKey: string;
	descriptionKey: string;
}> = [
	{
		skill: ORCHESTRATION_PRESET,
		titleKey: "skills.presets.orchestration",
		descriptionKey: "skills.design.presetHint",
	},
];
