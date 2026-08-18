import type { SkillRecord } from "@/lib/skills-api";
import orchestrationSkillMarkdown from "./presets/dcc-orchestration/SKILL.md?raw";

function frontmatterValue(markdown: string, key: string): string {
	const frontmatter = markdown.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/)?.[1];
	if (!frontmatter) {
		throw new Error("The orchestration preset has no frontmatter");
	}
	const prefix = `${key}:`;
	const line = frontmatter
		.split(/\r?\n/)
		.find((candidate) => candidate.startsWith(prefix));
	if (!line) {
		throw new Error(`The orchestration preset has no ${key}`);
	}
	return line.slice(prefix.length).trim();
}

function skillBody(markdown: string): string {
	return markdown.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").trim();
}

export const ORCHESTRATION_PRESET: SkillRecord = {
	name: frontmatterValue(orchestrationSkillMarkdown, "name"),
	description: frontmatterValue(orchestrationSkillMarkdown, "description"),
	body: skillBody(orchestrationSkillMarkdown),
	targetAgents: ["codex"],
	disableModelInvocation: false,
	scope: "project",
};
