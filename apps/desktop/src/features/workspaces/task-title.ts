const MAX_TASK_TITLE_LENGTH = 56;
const MAX_TASK_TITLE_WORDS = 8;
const AUTOMATIC_TASK_TITLES = new Set(["new task", "nova tarefa"]);

const REQUEST_PREFIXES = [
	/^por\s+favor[,\s:]*/iu,
	/^preciso\s+que\s+(?:você|voce)\s+/iu,
	/^quero\s+que\s+(?:você|voce)\s+/iu,
	/^(?:você|voce)\s+pode\s+/iu,
	/^pode\s+/iu,
	/^please[,\s:]*/iu,
	/^i\s+need\s+you\s+to\s+/iu,
	/^i\s+want\s+you\s+to\s+/iu,
	/^(?:can|could|would)\s+you\s+/iu,
];

function capitalize(value: string) {
	const [first, ...rest] = Array.from(value);
	return first ? `${first.toLocaleUpperCase()}${rest.join("")}` : value;
}

/** Recognizes localized placeholders that must be replaced by the first prompt. */
export function isAutomaticTaskTitle(title: string | null | undefined): boolean {
	const normalized = title?.replace(/\s+/gu, " ").trim().toLocaleLowerCase() ?? "";
	return normalized.length === 0 || AUTOMATIC_TASK_TITLES.has(normalized);
}

/** Builds an immediate, local task title from the first meaningful prompt. */
export function deriveTaskTitle(
	prompt: string,
	fallbackTitle = "Nova tarefa",
): string {
	let candidate = prompt
		.replace(/```[\s\S]*?```/gu, " ")
		.split(/\r?\n/u)
		.map((line) => line.trim())
		.find((line) => line.length > 0) ?? "";
	candidate = candidate
		.replace(/^\/(?:plan|compact|review|spec)\b\s*/iu, "")
		.replace(/^[-*#>\d.)\s]+/u, "")
		.replace(/\s+/gu, " ")
		.trim();

	for (const prefix of REQUEST_PREFIXES) {
		const withoutPrefix = candidate.replace(prefix, "").trim();
		if (withoutPrefix !== candidate) {
			candidate = withoutPrefix;
			break;
		}
	}

	candidate = candidate
		.split(/(?<=[.!?])\s/u, 1)[0]
		.replace(/[.!?,;:]+$/u, "")
		.trim();
	const words = candidate.split(/\s+/u).filter(Boolean);
	let title = words.slice(0, MAX_TASK_TITLE_WORDS).join(" ");
	if (title.length > MAX_TASK_TITLE_LENGTH) {
		title = `${title.slice(0, MAX_TASK_TITLE_LENGTH - 1).trimEnd()}…`;
	} else if (words.length > MAX_TASK_TITLE_WORDS) {
		title = `${title}…`;
	}

	return capitalize(title || fallbackTitle);
}
