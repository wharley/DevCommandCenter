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
	specRelativePath: string | null;
	specHash: string | null;
	summary: string | null;
	criteria: MissionValidationCriterionResult[];
	rawJson: string;
};

export type MissionResumeCriterion = MissionAcceptanceCriterion & {
	status: MissionValidationStatus;
	evidence: string;
	nextAction: string;
};

export type MissionResumeContext = {
	state: "needs_validation" | "stale_validation" | "pending" | "complete";
	reason: string | null;
	criteria: MissionResumeCriterion[];
};

const FRONTMATTER_RE = /^---\n([\s\S]*?)\n---(?:\n|$)/;
const CRITERION_ID_RE = /\b[A-Z]{1,8}-\d+\b/;
const MARKDOWN_CRITERIA_HEADING_RE =
	/^\s{0,3}#{1,6}\s+acceptance criteria\s*$/i;
const MARKDOWN_HEADING_RE = /^\s{0,3}#{1,6}\s+/;
const JSON_FENCE_RE = /```(?:json)?\s*([\s\S]*?)```/gi;

export function computeMissionSpecHash(specMarkdown: string) {
	let hash = 0x811c9dc5;
	for (let index = 0; index < specMarkdown.length; index += 1) {
		hash ^= specMarkdown.charCodeAt(index);
		hash = Math.imul(hash, 0x01000193);
	}
	return `fnv1a32:${(hash >>> 0).toString(16).padStart(8, "0")}`;
}

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
	planSteps?: Array<{ criteria?: string[] | null }>,
): MissionAcceptanceCriterionCoverage[] {
	const normalizedPlan = planMarkdown.toLowerCase();
	const structuredIds = new Set(
		(planSteps ?? []).flatMap((step) =>
			(step.criteria ?? []).map((criterion) => criterion.toLowerCase()),
		),
	);
	return criteria.map((criterion) => ({
		...criterion,
		covered:
			structuredIds.has(criterion.id.toLowerCase()) ||
			normalizedPlan.includes(criterion.id.toLowerCase()),
	}));
}

export function buildMissionValidationPrompt({
	specRelativePath,
	specMarkdown,
	planMarkdown,
}: {
	specRelativePath?: string | null;
	specMarkdown: string;
	planMarkdown?: string | null;
}) {
	const normalizedSpec = specMarkdown.trim();
	const normalizedPlan = planMarkdown?.trim() ?? "";
	const normalizedSpecPath = specRelativePath?.trim() ?? null;
	const specHash = computeMissionSpecHash(specMarkdown);
	return [
		"VALIDATE THIS MISSION AGAINST ITS SPEC.",
		"",
		"Do not modify files. Inspect the repository and run only the checks needed to evaluate the acceptance criteria.",
		"Return a concise validation report with one row per acceptance criterion using: PASS, FAIL, or UNKNOWN.",
		"For every FAIL or UNKNOWN, include the evidence or missing evidence and the smallest next action.",
		"End with a fenced JSON block that exactly follows this shape:",
		'{"dccMissionValidation":true,"specRelativePath":"...","specHash":"...","summary":"...","criteria":[{"id":"AC-1","status":"PASS","evidence":"...","nextAction":"..."}]}',
		...(normalizedSpecPath
			? ["", `Spec relative path for the JSON: ${normalizedSpecPath}`]
			: []),
		`Spec hash for the JSON: ${specHash}`,
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

export function buildMissionReanchorPrompt({
	specMarkdown,
	planMarkdown,
	validationJson,
}: {
	specMarkdown: string;
	planMarkdown?: string | null;
	validationJson?: string | null;
}) {
	const normalizedSpec = specMarkdown.trim();
	const normalizedPlan = planMarkdown?.trim() ?? "";
	const normalizedValidation = validationJson?.trim() ?? "";
	const resumeContext = buildMissionResumeContext({
		specMarkdown,
		validationJson: normalizedValidation,
	});
	const renderedResumeContext = resumeContext
		? renderMissionResumeContext(resumeContext)
		: null;
	return [
		"RE-ANCHOR THIS SESSION TO THE CURRENT MISSION STATE.",
		"",
		"Do not implement yet. Do not modify files.",
		"Use this message only to restore the mission requirements, current plan, and validation state in your working context.",
		"After reading, reply with a concise summary of the next pending work and any unresolved acceptance criteria.",
		"",
		"MISSION SPEC:",
		normalizedSpec,
		...(normalizedPlan
			? [
					"",
					"ACTIVE PLAN:",
					normalizedPlan,
				]
			: []),
		...(normalizedValidation
			? [
					"",
					"SAVED VALIDATION VERDICT:",
					normalizedValidation,
				]
			: []),
		...(renderedResumeContext
			? [
					"",
					"RESUME CONTEXT:",
					renderedResumeContext,
				]
			: []),
	].join("\n");
}

export function buildMissionContinueCriterionPrompt({
	specMarkdown,
	planMarkdown,
	validationJson,
	criterion,
}: {
	specMarkdown: string;
	planMarkdown?: string | null;
	validationJson?: string | null;
	criterion: MissionResumeCriterion;
}) {
	const normalizedSpec = specMarkdown.trim();
	const normalizedPlan = planMarkdown?.trim() ?? "";
	const normalizedValidation = validationJson?.trim() ?? "";
	const description = criterion.description.trim();
	const evidence = criterion.evidence.trim();
	const nextAction = criterion.nextAction.trim();
	return [
		"CONTINUE THE NEXT PENDING MISSION CRITERION.",
		"",
		"Focus only on the next pending acceptance criterion below.",
		"Implement the smallest change set that moves this criterion toward PASS.",
		"After implementing, summarize what changed and what still remains if the criterion is not fully satisfied.",
		"",
		`TARGET CRITERION: ${criterion.id} [${criterion.status}]`,
		...(description ? [`Description: ${description}`] : []),
		...(nextAction ? [`Suggested next action: ${nextAction}`] : []),
		...(evidence ? [`Current evidence or gap: ${evidence}`] : []),
		"",
		"MISSION SPEC:",
		normalizedSpec,
		...(normalizedPlan
			? [
					"",
					"ACTIVE PLAN:",
					normalizedPlan,
				]
			: []),
		...(normalizedValidation
			? [
					"",
					"SAVED VALIDATION VERDICT:",
					normalizedValidation,
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

export function buildMissionResumeContext({
	specMarkdown,
	validationJson,
}: {
	specMarkdown: string;
	validationJson?: string | null;
}): MissionResumeContext | null {
	const criteria = parseMissionAcceptanceCriteria(specMarkdown);
	if (criteria.length === 0) {
		return null;
	}

	const normalizedValidation = validationJson?.trim() ?? "";
	const validationReport = normalizedValidation
		? parseMissionValidationReport(normalizedValidation)
		: null;
	const currentSpecHash = computeMissionSpecHash(specMarkdown);
	const validationIssue = validationReport
		? getValidationFreshnessIssue(validationReport.specHash, currentSpecHash)
		: null;

	if (!validationReport || validationIssue) {
		const reason =
			validationIssue ?? "No saved validation verdict is available.";
		return {
			state: validationIssue ? "stale_validation" : "needs_validation",
			reason,
			criteria: criteria.map((criterion) => ({
				...criterion,
				status: "UNKNOWN",
				evidence: "",
				nextAction: "Validate this acceptance criterion.",
			})),
		};
	}

	const criteriaById = new Map(
		criteria.map((criterion) => [criterion.id.toLowerCase(), criterion]),
	);
	const validationById = new Map(
		validationReport.criteria.map((criterion) => [
			criterion.id.toLowerCase(),
			criterion,
		]),
	);
	const pendingCriteria: MissionResumeCriterion[] = [
		...validationReport.criteria
			.filter((criterion) => criterion.status !== "PASS")
			.map((criterion) => {
				const specCriterion = criteriaById.get(criterion.id.toLowerCase());
				return {
					id: criterion.id,
					description: specCriterion?.description ?? "",
					status: criterion.status,
					evidence: criterion.evidence,
					nextAction: criterion.nextAction,
				};
			}),
		...criteria
			.filter((criterion) => !validationById.has(criterion.id.toLowerCase()))
			.map<MissionResumeCriterion>((criterion) => ({
				...criterion,
				status: "UNKNOWN",
				evidence: "",
				nextAction: "Validate this acceptance criterion.",
			})),
	];

	if (pendingCriteria.length === 0) {
		return {
			state: "complete",
			reason:
				"Saved validation marks all known acceptance criteria as PASS. Continue only after checking whether the spec defines another phase or new acceptance criteria.",
			criteria: [],
		};
	}

	return {
		state: "pending",
		reason: "Next pending acceptance criteria from the saved validation:",
		criteria: pendingCriteria,
	};
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
		specRelativePath:
			typeof parsed.specRelativePath === "string"
				? parsed.specRelativePath.trim()
				: null,
		specHash:
			typeof parsed.specHash === "string" ? parsed.specHash.trim() : null,
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

function renderMissionResumeContext(context: MissionResumeContext) {
	return [
		context.reason,
		...(context.criteria.length > 0
			? [
					context.state === "pending"
						? "Next pending acceptance criteria:"
						: "Next pending acceptance criteria to confirm:",
				]
			: []),
		...context.criteria.map((criterion) => {
			const description = criterion.description
				? ` ${criterion.description}`
				: "";
			const nextAction = criterion.nextAction
				? ` Next: ${criterion.nextAction}`
				: "";
			const evidence = criterion.evidence
				? ` Evidence: ${criterion.evidence}`
				: "";
			return `- ${criterion.id} [${criterion.status}]:${description}${nextAction}${evidence}`;
		}),
	]
		.filter((line): line is string => Boolean(line))
		.join("\n");
}

function getValidationFreshnessIssue(
	specHash: string | null,
	currentSpecHash: string,
) {
	if (!specHash) {
		return `Saved validation has no spec hash (${currentSpecHash} expected); treat it as historical context only.`;
	}
	if (specHash !== currentSpecHash) {
		return `Saved validation is stale (${specHash} != ${currentSpecHash}); treat it as historical context only.`;
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
