export const EFFORT_RANK: Record<string, number> = {
	minimal: 0,
	low: 1,
	balanced: 2,
	medium: 2,
	high: 3,
	xhigh: 4,
	max: 5,
	ultrathink: 6,
};

export const DEFAULT_EFFORT_LEVELS = ["low", "medium", "high"];
export const DEFAULT_EFFORT_LEVEL = "medium";

export const EFFORT_DISPLAY: Record<string, { label: string; icon: string }> = {
	minimal: { label: "Minimal", icon: "minimal" },
	low: { label: "Low", icon: "low" },
	balanced: { label: "Balanced", icon: "medium" },
	medium: { label: "Medium", icon: "medium" },
	high: { label: "High", icon: "high" },
	xhigh: { label: "Extra High", icon: "max" },
	max: { label: "Max", icon: "max" },
	ultrathink: { label: "Ultrathink", icon: "max" },
};

export function getEffortDisplay(id: string) {
	return EFFORT_DISPLAY[id] ?? {
		label: id.charAt(0).toUpperCase() + id.slice(1),
		icon: "medium",
	};
}

function effortRank(level: string) {
	return EFFORT_RANK[level] ?? EFFORT_RANK.medium;
}

export function clampEffort(effort: string, supported: string[]): string {
	if (supported.length === 0) return DEFAULT_EFFORT_LEVEL;
	if (supported.includes(effort)) return effort;

	const ranked = supported
		.map((level) => ({ level, rank: effortRank(level) }))
		.sort((left, right) => left.rank - right.rank);
	const targetRank = effortRank(effort);
	const floor = [...ranked].reverse().find((entry) => entry.rank <= targetRank);
	const ceil = ranked.find((entry) => entry.rank >= targetRank);

	return floor?.level ?? ceil?.level ?? supported[0] ?? DEFAULT_EFFORT_LEVEL;
}

export function highestAvailableEffort(supported: string[]): string {
	const ranked = [...supported].sort((left, right) => effortRank(left) - effortRank(right));
	return ranked[ranked.length - 1] ?? DEFAULT_EFFORT_LEVEL;
}

export function isUltrathinkPrompt(prompt: string): boolean {
	return /\bultrathink\b/i.test(prompt);
}

export function resolveEffectiveEffort(input: {
	selectedEffort: string;
	supportedEfforts: string[];
	ultrathinkSelected: boolean;
	rawPrompt: string;
}) {
	const { selectedEffort, supportedEfforts, ultrathinkSelected, rawPrompt } = input;
	if (ultrathinkSelected || isUltrathinkPrompt(rawPrompt)) {
		return highestAvailableEffort(supportedEfforts);
	}
	return clampEffort(selectedEffort, supportedEfforts);
}
