import { invoke } from "@tauri-apps/api/core";

/** Target agents a source skill compiles to. `agents` covers Codex/Droid via AGENTS.md. */
export type SkillTargetAgent = "claude" | "agents";

/** Provider-neutral skill. Source of truth lives in `.devcommandcenter/skills/`. */
export type SkillRecord = {
	name: string;
	description: string;
	body: string;
	targetAgents: SkillTargetAgent[];
	disableModelInvocation: boolean;
	scope: string;
};

export function listSkills(projectRoot: string) {
	return invoke<SkillRecord[]>("skills_list", { projectRoot });
}

export function saveSkill(projectRoot: string, skill: SkillRecord) {
	return invoke<void>("skills_save", { projectRoot, skill });
}

export function deleteSkill(projectRoot: string, name: string) {
	return invoke<void>("skills_delete", { projectRoot, name });
}

/**
 * Projects the neutral source into each agent's native format inside `targetRoot`
 * (the active worktree, where agents run): `.claude/skills/` (copy) + `AGENTS.md` (flattened).
 */
export function compileSkills(projectRoot: string, targetRoot: string) {
	return invoke<void>("skills_compile", { projectRoot, targetRoot });
}
