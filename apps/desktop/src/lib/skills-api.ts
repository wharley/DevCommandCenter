import { invoke } from "@tauri-apps/api/core";

/** Target agents a source skill compiles to. `agents` is the legacy always-on AGENTS.md target. */
export type SkillTargetAgent =
	| "claude"
	| "codex"
	| "agents"
	| "gemini"
	| "cursor";

/** Provider-neutral skill. Source of truth lives in `.devcommandcenter/skills/`. */
export type SkillRecord = {
	name: string;
	description: string;
	body: string;
	targetAgents: SkillTargetAgent[];
	disableModelInvocation: boolean;
	scope: string;
};

export type SkillContextDetection = {
	id: string;
	kind:
		| "dcc_source"
		| "instructions_file"
		| "claude_skills"
		| "cursor_rules"
		| "codex_skills";
	title: string;
	relativePath: string;
	rootKind: "project_root" | "target_root";
	count: number;
	managedCount: number;
	externalCount: number;
	hasDccBlock: boolean;
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
 * (the active worktree, where agents run): native skill directories plus legacy
 * always-on instruction targets such as `AGENTS.md`.
 */
export function compileSkills(projectRoot: string, targetRoot: string) {
	return invoke<void>("skills_compile", { projectRoot, targetRoot });
}

export function detectSkillContext(
	projectRoot: string,
	targetRoot: string | null,
) {
	return invoke<SkillContextDetection[]>("skills_detect_context", {
		projectRoot,
		targetRoot,
	});
}
