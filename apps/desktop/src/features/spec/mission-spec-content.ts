export type MissionAcceptanceCriterion = {
	id: string;
	description: string;
};

export type MissionAcceptanceCriterionCoverage = MissionAcceptanceCriterion & {
	covered: boolean;
};

export type MissionValidationStatus = "PASS" | "FAIL" | "UNKNOWN";

export type MissionValidationCriterionResult = {
	id: string;
	status: MissionValidationStatus;
	evidence: string;
	nextAction: string;
};

export type ParsedMissionValidationReport = {
	summary: string | null;
	criteria: MissionValidationCriterionResult[];
	rawJson: string;
};

const FRONTMATTER_RE = /^---\n([\s\S]*?)\n---(?:\n|$)/;
const CRITERION_ID_RE = /\b[A-Z]{1,8}-\d+\b/;
const MARKDOWN_CRITERIA_HEADING_RE =
	/^\s{0,3}#{1,6}\s+acceptance criteria\s*$/i;
const MARKDOWN_HEADING_RE = /^\s{0,3}#{1,6}\s+/;
const JSON_FENCE_RE = /```(?:json)?\s*([\s\S]*?)```/gi;

export function parseMissionAcceptanceCriteria(
	specMarkdown: string,
): MissionAcceptanceCriterion[] {
	const criteria = [
		...parseFrontmatterCriteria(specMarkdown),
		...parseMarkdownCriteria(specMarkdown),
	];
	const byId = new Map<string, MissionAcceptanceCriterion>();
	for (const criterion of criteria) {
		if (!criterion.id) {
			continue;
		}
		if (!byId.has(criterion.id)) {
			byId.set(criterion.id, criterion);
		}
	}
	return [...byId.values()];
}

export function buildMissionAcceptanceCriteriaCoverage(
	criteria: MissionAcceptanceCriterion[],
	planMarkdown: string,
): MissionAcceptanceCriterionCoverage[] {
	const normalizedPlan = planMarkdown.toLowerCase();
	return criteria.map((criterion) => ({
		...criterion,
		covered: normalizedPlan.includes(criterion.id.toLowerCase()),
	}));
}

export function buildMissionValidationPrompt({
	specMarkdown,
	planMarkdown,
}: {
	specMarkdown: string;
	planMarkdown?: string | null;
}) {
	const normalizedSpec = specMarkdown.trim();
	const normalizedPlan = planMarkdown?.trim() ?? "";
	return [
		"VALIDATE THIS MISSION AGAINST ITS SPEC.",
		"",
		"Do not modify files. Inspect the repository and run only the checks needed to evaluate the acceptance criteria.",
		"Return a concise validation report with one row per acceptance criterion using: PASS, FAIL, or UNKNOWN.",
		"For every FAIL or UNKNOWN, include the evidence or missing evidence and the smallest next action.",
		"End with a fenced JSON block that exactly follows this shape:",
		'{"dccMissionValidation":true,"summary":"...","criteria":[{"id":"AC-1","status":"PASS","evidence":"...","nextAction":"..."}]}',
		"",
		"SPEC:",
		normalizedSpec,
		...(normalizedPlan
			? [
					"",
					"CURRENT PLAN CONTEXT:",
					normalizedPlan,
				]
			: []),
	].join("\n");
}

export function parseMissionValidationReport(
	content: string,
): ParsedMissionValidationReport | null {
	for (const candidate of extractJsonCandidates(content)) {
		const parsed = tryParseMissionValidationJson(candidate);
		if (parsed) {
			return parsed;
		}
	}
	return null;
}

function extractJsonCandidates(content: string) {
	const candidates: string[] = [];
	for (const match of content.matchAll(JSON_FENCE_RE)) {
		const value = match[1]?.trim();
		if (value) {
			candidates.push(value);
		}
	}

	const trimmed = content.trim();
	if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
		candidates.push(trimmed);
	}

	return candidates;
}

function tryParseMissionValidationJson(
	rawJson: string,
): ParsedMissionValidationReport | null {
	let parsed: unknown;
	try {
		parsed = JSON.parse(rawJson);
	} catch {
		return null;
	}

	if (!isRecord(parsed) || parsed.dccMissionValidation !== true) {
		return null;
	}

	const criteriaInput = Array.isArray(parsed.criteria) ? parsed.criteria : [];
	const criteria = criteriaInput
		.map(normalizeValidationCriterionResult)
		.filter((criterion): criterion is MissionValidationCriterionResult =>
			Boolean(criterion),
		);
	if (criteria.length === 0) {
		return null;
	}

	return {
		summary: typeof parsed.summary === "string" ? parsed.summary.trim() : null,
		criteria,
		rawJson,
	};
}

function normalizeValidationCriterionResult(
	input: unknown,
): MissionValidationCriterionResult | null {
	if (!isRecord(input)) {
		return null;
	}

	const id = typeof input.id === "string" ? input.id.trim() : "";
	const status = normalizeValidationStatus(input.status);
	if (!id || !status) {
		return null;
	}

	return {
		id,
		status,
		evidence: typeof input.evidence === "string" ? input.evidence.trim() : "",
		nextAction:
			typeof input.nextAction === "string" ? input.nextAction.trim() : "",
	};
}

function normalizeValidationStatus(
	value: unknown,
): MissionValidationStatus | null {
	if (typeof value !== "string") {
		return null;
	}
	const normalized = value.trim().toUpperCase();
	if (
		normalized === "PASS" ||
		normalized === "FAIL" ||
		normalized === "UNKNOWN"
	) {
		return normalized;
	}
	return null;
}

function parseFrontmatterCriteria(
	specMarkdown: string,
): MissionAcceptanceCriterion[] {
	const match = specMarkdown.match(FRONTMATTER_RE);
	const frontmatter = match?.[1] ?? "";
	if (!frontmatter.includes("acceptance_criteria:")) {
		return [];
	}

	const criteria: MissionAcceptanceCriterion[] = [];
	let current: Partial<MissionAcceptanceCriterion> | null = null;
	for (const rawLine of frontmatter.split("\n")) {
		const line = rawLine.trim();
		const idMatch = line.match(/^(?:-\s*)?id:\s*["']?([^"']+)["']?$/i);
		if (idMatch) {
			if (current?.id) {
				criteria.push(normalizeCriterion(current));
			}
			current = { id: idMatch[1]?.trim() ?? "" };
			continue;
		}

		const descriptionMatch = line.match(
			/^description:\s*["']?([\s\S]*?)["']?$/i,
		);
		if (descriptionMatch && current) {
			current.description = descriptionMatch[1]?.trim() ?? "";
		}
	}

	if (current?.id) {
		criteria.push(normalizeCriterion(current));
	}

	return criteria.filter((criterion) => criterion.id.length > 0);
}

function parseMarkdownCriteria(specMarkdown: string): MissionAcceptanceCriterion[] {
	const criteria: MissionAcceptanceCriterion[] = [];
	let inCriteriaSection = false;
	for (const rawLine of specMarkdown.split("\n")) {
		if (MARKDOWN_CRITERIA_HEADING_RE.test(rawLine)) {
			inCriteriaSection = true;
			continue;
		}
		if (inCriteriaSection && MARKDOWN_HEADING_RE.test(rawLine)) {
			break;
		}
		if (!inCriteriaSection) {
			continue;
		}

		const text = rawLine
			.trim()
			.replace(/^[-*+]\s+(?:\[[ xX~-]\]\s+)?/, "")
			.trim();
		const id = text.match(CRITERION_ID_RE)?.[0] ?? "";
		if (!id) {
			continue;
		}
		const description = text
			.replace(new RegExp(`^${escapeRegExp(id)}\\s*:?\\s*`), "")
			.trim();
		criteria.push({ id, description });
	}
	return criteria;
}

function normalizeCriterion(
	criterion: Partial<MissionAcceptanceCriterion>,
): MissionAcceptanceCriterion {
	return {
		id: criterion.id?.trim() ?? "",
		description: criterion.description?.trim() ?? "",
	};
}

function escapeRegExp(value: string) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
