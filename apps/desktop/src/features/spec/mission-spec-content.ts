export type MissionAcceptanceCriterion = {
	id: string;
	description: string;
};

export type MissionValidationCheck = {
	text: string;
};

export type MissionValidationPersistence = "manual" | "auto";

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
	persistenceMode: MissionValidationPersistence | null;
	persistedAt: string | null;
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
const MARKDOWN_VALIDATION_CHECKS_HEADING_RE =
	/^\s{0,3}#{1,6}\s+validation checks\s*$/i;
const MARKDOWN_HEADING_RE = /^\s{0,3}#{1,6}\s+/;
const JSON_FENCE_RE = /```(?:json)?\s*([\s\S]*?)```/gi;
const VALIDATION_PROFILE_CHECKS: Record<string, string[]> = {
	node: [
		"Run the repository's standard Node or workspace typecheck command when one exists.",
		"Run the repository's standard Node or workspace test command when one exists.",
	],
	react: [
		"Exercise the affected UI flow manually and confirm the observable result.",
		"Run the frontend typecheck or test command used by the repository when one exists.",
	],
	rust: [
		"Run the relevant cargo check command for the affected crate or workspace.",
		"Run the relevant cargo test command when tests exist for the affected crate or workspace.",
	],
	tauri: [
		"Run the desktop frontend check and the Rust backend check used by the repository.",
		"Exercise the affected desktop flow manually before concluding PASS.",
	],
};

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

export function parseMissionValidationChecks(
	specMarkdown: string,
): MissionValidationCheck[] {
	return dedupeMissionValidationChecks([
		...parseFrontmatterValidationChecks(specMarkdown),
		...parseMarkdownValidationChecks(specMarkdown),
	]);
}

export function parseMissionValidationProfiles(specMarkdown: string): string[] {
	const profiles = [
		...parseFrontmatterValidationProfileList(specMarkdown),
		...parseFrontmatterValidationProfileValue(specMarkdown),
	];
	const deduped = new Set<string>();
	for (const profile of profiles) {
		const normalized = profile.trim().toLowerCase();
		if (!normalized || !(normalized in VALIDATION_PROFILE_CHECKS)) {
			continue;
		}
		deduped.add(normalized);
	}
	return [...deduped];
}

export function parseMissionSuggestedValidationChecks(
	specMarkdown: string,
): MissionValidationCheck[] {
	const profiles = parseMissionValidationProfiles(specMarkdown);
	if (profiles.length === 0) {
		return [];
	}
	return dedupeMissionValidationChecks(
		profiles.flatMap((profile) =>
			(VALIDATION_PROFILE_CHECKS[profile] ?? []).map((text) => ({ text })),
		),
	);
}

export function parseMissionValidationPersistence(
	specMarkdown: string,
): MissionValidationPersistence {
	const value = parseFrontmatterScalarValue(specMarkdown, "validation_persistence");
	if (value === "auto") {
		return "auto";
	}
	return "manual";
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
	const validationChecks = parseMissionValidationChecks(specMarkdown);
	const suggestedChecks =
		validationChecks.length === 0
			? parseMissionSuggestedValidationChecks(specMarkdown)
			: [];
	return [
		"VALIDATE THIS MISSION AGAINST ITS SPEC.",
		"",
		"Do not modify files. Inspect the repository and run only the checks needed to evaluate the acceptance criteria.",
		...(validationChecks.length > 0
			? [
					"Prioritize these validation checks declared by the spec before concluding PASS:",
					...validationChecks.map((check) => `- ${check.text}`),
				]
			: []),
		...(suggestedChecks.length > 0
			? [
					"No explicit validation checks were declared. Start from these defaults suggested by the spec validation profiles:",
					...suggestedChecks.map((check) => `- ${check.text}`),
				]
			: []),
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

export function buildMissionValidationSavePayload({
	rawJson,
	mode,
	savedAt,
}: {
	rawJson: string;
	mode: MissionValidationPersistence;
	savedAt?: string;
}): string | null {
	let parsed: unknown;
	try {
		parsed = JSON.parse(rawJson);
	} catch {
		return null;
	}

	if (!isRecord(parsed) || parsed.dccMissionValidation !== true) {
		return null;
	}

	return JSON.stringify({
		...parsed,
		dccPersistenceMode: mode,
		dccSavedAt: (savedAt ?? new Date().toISOString()).trim(),
	});
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
		persistenceMode: normalizeValidationPersistence(parsed.dccPersistenceMode),
		persistedAt:
			typeof parsed.dccSavedAt === "string" ? parsed.dccSavedAt.trim() : null,
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

function normalizeValidationPersistence(
	value: unknown,
): MissionValidationPersistence | null {
	return value === "manual" || value === "auto" ? value : null;
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

function parseFrontmatterValidationChecks(
	specMarkdown: string,
): MissionValidationCheck[] {
	const match = specMarkdown.match(FRONTMATTER_RE);
	const frontmatter = match?.[1] ?? "";
	if (!frontmatter.includes("validation_checks:")) {
		return [];
	}

	const checks: MissionValidationCheck[] = [];
	let inChecks = false;
	for (const rawLine of frontmatter.split("\n")) {
		const line = rawLine.trim();
		if (!inChecks) {
			if (/^validation_checks:\s*$/i.test(line)) {
				inChecks = true;
			}
			continue;
		}

		if (!line) {
			continue;
		}
		if (/^[A-Za-z0-9_-]+:\s*/.test(line)) {
			break;
		}

		const checkMatch = line.match(/^-\s+(.+)$/);
		if (checkMatch) {
			checks.push({ text: checkMatch[1]?.trim() ?? "" });
		}
	}

	return checks.filter((check) => check.text.length > 0);
}

function parseFrontmatterValidationProfileList(specMarkdown: string): string[] {
	const match = specMarkdown.match(FRONTMATTER_RE);
	const frontmatter = match?.[1] ?? "";
	if (!frontmatter.includes("validation_profiles:")) {
		return [];
	}

	const profiles: string[] = [];
	let inProfiles = false;
	for (const rawLine of frontmatter.split("\n")) {
		const line = rawLine.trim();
		if (!inProfiles) {
			if (/^validation_profiles:\s*$/i.test(line)) {
				inProfiles = true;
			}
			continue;
		}

		if (!line) {
			continue;
		}
		if (/^[A-Za-z0-9_-]+:\s*/.test(line)) {
			break;
		}

		const profileMatch = line.match(/^-\s+(.+)$/);
		if (profileMatch) {
			profiles.push(profileMatch[1]?.trim() ?? "");
		}
	}

	return profiles.filter((profile) => profile.length > 0);
}

function parseFrontmatterValidationProfileValue(specMarkdown: string): string[] {
	const value = parseFrontmatterScalarValue(specMarkdown, "validation_profile");
	return value ? [value] : [];
}

function parseFrontmatterScalarValue(
	specMarkdown: string,
	key: string,
): string | null {
	const match = specMarkdown.match(FRONTMATTER_RE);
	const frontmatter = match?.[1] ?? "";
	const pattern = new RegExp(`^${escapeRegExp(key)}:\\s*["']?([^"'\\n]+)["']?\\s*$`, "im");
	const valueMatch = frontmatter.match(pattern);
	return valueMatch?.[1]?.trim() ?? null;
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

function parseMarkdownValidationChecks(
	specMarkdown: string,
): MissionValidationCheck[] {
	const checks: MissionValidationCheck[] = [];
	let inChecksSection = false;
	for (const rawLine of specMarkdown.split("\n")) {
		if (MARKDOWN_VALIDATION_CHECKS_HEADING_RE.test(rawLine)) {
			inChecksSection = true;
			continue;
		}
		if (inChecksSection && MARKDOWN_HEADING_RE.test(rawLine)) {
			break;
		}
		if (!inChecksSection) {
			continue;
		}

		const text = rawLine.trim().replace(/^[-*+]\s+/, "").trim();
		if (!text) {
			continue;
		}
		checks.push({ text });
	}
	return checks;
}

function dedupeMissionValidationChecks(
	checks: MissionValidationCheck[],
): MissionValidationCheck[] {
	const byText = new Set<string>();
	const deduped: MissionValidationCheck[] = [];
	for (const check of checks) {
		const normalized = check.text.trim();
		if (!normalized) {
			continue;
		}
		const key = normalized.toLowerCase();
		if (byText.has(key)) {
			continue;
		}
		byText.add(key);
		deduped.push({ text: normalized });
	}
	return deduped;
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
