import { Check, Bot } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { SkillTargetAgent } from "@/lib/skills-api";

export const SKILL_TARGETS: Record<
	SkillTargetAgent,
	{ name: string; path: string }
> = {
	claude: { name: "Claude", path: ".claude/skills" },
	codex: { name: "Codex", path: ".agents/skills" },
	agents: { name: "Droid · legacy", path: "AGENTS.md" },
	gemini: { name: "Gemini", path: "GEMINI.md" },
	cursor: { name: "Cursor", path: ".cursor/rules" },
};

export function SkillTargetPicker({
	value,
	onChange,
}: {
	value: SkillTargetAgent[];
	onChange: (value: SkillTargetAgent[]) => void;
}) {
	const { t } = useTranslation("common");
	return (
		<div
			className="skills-target-grid"
			role="group"
			aria-label={t("skills.fields.targetAgents")}
		>
			{(
				Object.entries(SKILL_TARGETS) as [
					SkillTargetAgent,
					{ name: string; path: string },
				][]
			).map(([id, target]) => (
				<button
					key={id}
					type="button"
					className="skills-target"
					aria-pressed={value.includes(id)}
					aria-label={target.name}
					onClick={() =>
						onChange(
							value.includes(id)
								? value.filter((item) => item !== id)
								: [...value, id],
						)
					}
				>
					<Bot size={15} aria-hidden />
					<span>
						<strong>{target.name}</strong>
						<code>{target.path}</code>
					</span>
					<span className="skills-target-check">
						{value.includes(id) && <Check size={11} aria-hidden />}
					</span>
				</button>
			))}
		</div>
	);
}
