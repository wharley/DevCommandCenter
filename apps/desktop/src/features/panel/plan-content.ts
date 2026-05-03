import {
	extractCommandsFromPlan,
	extractPendingCommands,
} from "../../../../../lib/command-detector";

export type PendingCommand = {
	id: string;
	command: string;
	description?: string;
	source: "plan" | "code" | "file";
	confirmedAt: string | null;
};

export type ParsedPlanStepStatus = "completed" | "in_progress" | "pending";

export type ParsedPlanStep = {
	raw: string;
	text: string;
	status: ParsedPlanStepStatus;
	index: number;
};

export type ParsedPlanContent = {
	title: string;
	summary: string | null;
	steps: ParsedPlanStep[];
	approvedPrompts: PendingCommand[];
	rawMarkdown: string;
	markdown: string;
	isPlanLike: boolean;
	canCollapse: boolean;
	source: "json" | "markdown" | "plain";
};

type StructuredPlan = {
	title?: string;
	summary?: string;
	steps?: Array<string | Record<string, unknown>>;
};

const PLAN_HEADING_RE = /^#{1,6}\s+(.*)$/;
const CHECKLIST_STEP_RE =
	/^(?:[-*+]|(?:\d+\.))\s+\[(?<check>[ xX~\-\/])\]\s+(?<text>.+)$/;
const LIST_STEP_RE = /^(?:[-*+]|(?:\d+\.))\s+(?<text>.+)$/;
const HEADING_SECTION_RE =
	/^#{1,3}\s*(summary|resumo|overview|objective|objectives|steps|passos|plan|plano|approved prompts|prompts|commands?)\s*$/i;

export function parsePlanContent(content: string): ParsedPlanContent {
	const rawMarkdown = normalizePlanText(content);
	if (!rawMarkdown) {
		return {
			title: "Plan",
			summary: null,
			steps: [],
			approvedPrompts: [],
			rawMarkdown: "",
			markdown: "",
			isPlanLike: false,
			canCollapse: false,
			source: "plain",
		};
	}

	const structuredPlan = tryParseStructuredPlan(rawMarkdown);
	if (structuredPlan) {
		return buildStructuredPlanContent(structuredPlan, rawMarkdown);
	}

	return parseMarkdownPlanContent(rawMarkdown);
}

export function normalizePlanContentForExport(content: string) {
	const normalized = normalizePlanText(content);
	return normalized.length > 0 ? `${normalized}\n` : normalized;
}

export function buildPlanMarkdownFilename(content: string, fallback = "mission-plan.md") {
	const parsed = parsePlanContent(content);
	const slug = slugify(parsed.title);
	return slug ? `${slug}.md` : fallback;
}

export function stripDisplayedPlanMarkdown(planMarkdown: string): string {
	const lines = normalizePlanText(planMarkdown).split("\n");
	const sourceLines =
		lines[0] && /^\s{0,3}#{1,6}\s+/.test(lines[0]) ? lines.slice(1) : [...lines];
	while (sourceLines[0]?.trim().length === 0) {
		sourceLines.shift();
	}
	const firstHeadingMatch = sourceLines[0]?.match(/^\s{0,3}#{1,6}\s+(.+)$/);
	if (firstHeadingMatch?.[1]?.trim().toLowerCase() === "summary") {
		sourceLines.shift();
		while (sourceLines[0]?.trim().length === 0) {
			sourceLines.shift();
		}
	}
	return sourceLines.join("\n");
}

export function buildCollapsedPlanPreviewMarkdown(
	planMarkdown: string,
	options?: {
		maxLines?: number;
	},
): string {
	const maxLines = options?.maxLines ?? 8;
	const lines = stripDisplayedPlanMarkdown(planMarkdown)
		.trimEnd()
		.split("\n")
		.map((line) => line.trimEnd());
	const previewLines: string[] = [];
	let visibleLineCount = 0;
	let hasMoreContent = false;

	for (const line of lines) {
		const isVisibleLine = line.trim().length > 0;
		if (isVisibleLine && visibleLineCount >= maxLines) {
			hasMoreContent = true;
			break;
		}
		previewLines.push(line);
		if (isVisibleLine) {
			visibleLineCount += 1;
		}
	}

	while (previewLines.length > 0 && previewLines.at(-1)?.trim().length === 0) {
		previewLines.pop();
	}

	if (previewLines.length === 0) {
		return parsePlanContent(planMarkdown).title || "Plan preview unavailable.";
	}

	if (hasMoreContent) {
		previewLines.push("", "...");
	}

	return previewLines.join("\n");
}

export function downloadPlanAsTextFile(filename: string, contents: string) {
	const blob = new Blob([contents], { type: "text/markdown;charset=utf-8" });
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement("a");
	anchor.href = url;
	anchor.download = filename;
	anchor.rel = "noreferrer";
	document.body.appendChild(anchor);
	anchor.click();
	anchor.remove();
	URL.revokeObjectURL(url);
}

function buildStructuredPlanContent(
	plan: StructuredPlan,
	rawMarkdown: string,
): ParsedPlanContent {
	const title = normalizeTitle(plan.title) ?? "Plan";
	const summary = normalizeText(plan.summary);
	const steps = normalizeStructuredSteps(plan.steps ?? []);
	const markdown = buildMarkdownFromStructuredPlan({ title, summary, steps });
	const approvedPrompts = buildApprovedPrompts(summary, steps, rawMarkdown);
	return {
		title,
		summary,
		steps,
		approvedPrompts,
		rawMarkdown,
		markdown,
		isPlanLike: true,
		canCollapse: markdown.length > 900 || steps.length > 8,
		source: "json",
	};
}

function parseMarkdownPlanContent(rawMarkdown: string): ParsedPlanContent {
	const lines = rawMarkdown.split(/\r?\n/);
	const titleFromHeading = findFirstHeading(lines);
	const steps: ParsedPlanStep[] = [];
	const summaryLines: string[] = [];
	let collectingSummary = false;
	let collectingSteps = false;
	let summaryCaptured = false;

	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index] ?? "";
		const trimmed = line.trim();
		if (!trimmed) {
			if (collectingSummary && summaryLines.length > 0) {
				summaryLines.push("");
			}
			continue;
		}

		const headingMatch = PLAN_HEADING_RE.exec(trimmed);
		if (headingMatch) {
			const heading = headingMatch[1]?.trim() ?? "";
			const normalizedHeading = heading.toLowerCase();
			if (!titleFromHeading && index === 0) {
				continue;
			}
			collectingSummary = HEADING_SECTION_RE.test(trimmed) && isSummaryHeading(normalizedHeading);
			collectingSteps = HEADING_SECTION_RE.test(trimmed) && isStepsHeading(normalizedHeading);
			summaryCaptured = summaryCaptured || isSummaryHeading(normalizedHeading);
			continue;
		}

		const checklistMatch = CHECKLIST_STEP_RE.exec(trimmed);
		const listMatch = LIST_STEP_RE.exec(trimmed);
		if (checklistMatch || listMatch) {
			collectingSteps = true;
			collectingSummary = false;
			const raw = checklistMatch?.groups?.text ?? listMatch?.groups?.text ?? trimmed;
			steps.push({
				raw,
				text: cleanStepText(raw),
				status: statusFromCheckbox(checklistMatch?.groups?.check ?? null),
				index: steps.length,
			});
			continue;
		}

		if (collectingSteps && steps.length > 0) {
			const last = steps[steps.length - 1];
			last.raw = `${last.raw} ${trimmed}`.trim();
			last.text = cleanStepText(last.raw);
			continue;
		}

		if (!summaryCaptured || collectingSummary) {
			summaryLines.push(trimmed);
		}
	}

	const summary = normalizeText(summaryLines.join(" "));
	const approvedPrompts = buildApprovedPrompts(summary, steps, rawMarkdown);
	const markdown = rawMarkdown;
	const hasStructuralHints =
		steps.length > 0 ||
		Boolean(approvedPrompts.length) ||
		(Boolean(titleFromHeading) &&
			/plan|plano|mission|proposal|roadmap/i.test(titleFromHeading ?? "")) ||
		/^#{1,3}\s*(summary|resumo|steps|passos|plan|plano|approved prompts|commands?)\b/im.test(
			rawMarkdown,
		) ||
		/^\s*(?:[-*+]|\d+[.)])\s+\[[ xX~\-\/]\]/m.test(rawMarkdown);

	return {
		title: titleFromHeading ?? "Plan",
		summary: summary || null,
		steps,
		approvedPrompts,
		rawMarkdown,
		markdown,
		isPlanLike: hasStructuralHints,
		canCollapse: rawMarkdown.length > 900 || steps.length > 8 || rawMarkdown.split("\n").length > 18,
		source: "markdown",
	};
}

function buildApprovedPrompts(
	summary: string | null,
	steps: ParsedPlanStep[],
	rawMarkdown: string,
): PendingCommand[] {
	const commands = extractCommandsFromPlan({
		summary: summary ?? undefined,
		steps: steps.map((step) => ({
			title: step.text,
			description: step.raw,
		})),
	});
	const extraCommands = extractPendingCommands(rawMarkdown, "plan");
	const deduped = new Map<string, PendingCommand>();
	for (const command of [...commands, ...extraCommands]) {
		const key = command.command.toLowerCase();
		if (!deduped.has(key)) {
			deduped.set(key, command);
		}
	}
	return [...deduped.values()];
}

function buildMarkdownFromStructuredPlan(plan: {
	title: string;
	summary: string | null;
	steps: ParsedPlanStep[];
}) {
	const lines: string[] = [];
	lines.push(`# ${plan.title}`);
	if (plan.summary) {
		lines.push("");
		lines.push(plan.summary);
	}
	if (plan.steps.length > 0) {
		lines.push("");
		lines.push("## Steps");
		for (const step of plan.steps) {
			const marker =
				step.status === "completed"
					? "[x]"
					: step.status === "in_progress"
						? "[-]"
						: "[ ]";
			lines.push(`- ${marker} ${step.text}`);
		}
	}
	return lines.join("\n").trim();
}

function normalizeStructuredSteps(steps: Array<string | Record<string, unknown>>) {
	return steps
		.map((step, index) => {
			if (typeof step === "string") {
				const raw = normalizeText(step);
				if (!raw) {
					return null;
				}
				return {
					raw,
					text: cleanStepText(raw),
					status: statusFromRaw(raw),
					index,
				};
			}

			const text = normalizeText(
				typeof step.text === "string"
					? step.text
					: typeof step.title === "string"
						? step.title
						: typeof step.description === "string"
							? step.description
							: "",
			);
			if (!text) {
				return null;
			}
			return {
				raw: text,
				text: cleanStepText(text),
				status: statusFromRaw(
					typeof step.status === "string" ? step.status : text,
				),
				index,
			};
		})
		.filter((step): step is ParsedPlanStep => Boolean(step));
}

function statusFromRaw(raw: string): ParsedPlanStepStatus {
	const normalized = raw.toLowerCase();
	if (
		normalized.includes("[x]") ||
		normalized.includes("done") ||
		normalized.includes("completed") ||
		normalized.includes("complete") ||
		normalized.includes("finished")
	) {
		return "completed";
	}
	if (
		normalized.includes("[~]") ||
		normalized.includes("[-]") ||
		normalized.includes("in progress") ||
		normalized.includes("running") ||
		normalized.includes("working")
	) {
		return "in_progress";
	}
	return "pending";
}

function statusFromCheckbox(check: string | null): ParsedPlanStepStatus {
	if (!check) {
		return "pending";
	}
	const normalized = check.trim().toLowerCase();
	if (normalized === "x") {
		return "completed";
	}
	if (normalized === "~" || normalized === "-" || normalized === "/") {
		return "in_progress";
	}
	return "pending";
}

function findFirstHeading(lines: string[]) {
	for (const line of lines) {
		const match = PLAN_HEADING_RE.exec(line.trim());
		if (match) {
			return cleanTitle(match[1] ?? "");
		}
		if (line.trim()) {
			break;
		}
	}
	return null;
}

function isSummaryHeading(heading: string) {
	return /^(summary|resumo|overview|objective|objectives|goal|goals)$/i.test(heading);
}

function isStepsHeading(heading: string) {
	return /^(steps|passos|plan|plano|tasks?|actions?)$/i.test(heading);
}

function normalizePlanText(value: string) {
	return String(value ?? "").replace(/\r\n/g, "\n").trim();
}

function normalizeTitle(value: unknown) {
	const title = cleanTitle(normalizeText(value));
	return title || null;
}

function normalizeText(value: unknown) {
	if (typeof value !== "string") {
		return "";
	}
	return value.replace(/\r\n/g, "\n").trim();
}

function cleanTitle(value: string) {
	return value.replace(/^\s*#+\s*/, "").trim();
}

function cleanStepText(value: string) {
	return value
		.replace(/^\s*(?:[-*+]|\d+[.)])\s*/, "")
		.replace(/^\s*\[(?:x|X|~|-|\/)\]\s*/, "")
		.trim();
}

function slugify(value: string) {
	const normalized = value
		.normalize("NFKD")
		.replace(/[\u0300-\u036f]/g, "")
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
	return normalized;
}

function tryParseStructuredPlan(rawMarkdown: string): StructuredPlan | null {
	const unwrapped = unwrapCodeFence(rawMarkdown);
	const candidates = [rawMarkdown, unwrapped].filter(
		(value, index, array) => value.length > 0 && array.indexOf(value) === index,
	);
	for (const candidate of candidates) {
		const parsed = tryParseJsonPlan(candidate);
		if (parsed) {
			return parsed;
		}
	}
	return null;
}

function unwrapCodeFence(rawMarkdown: string) {
	const trimmed = rawMarkdown.trim();
	if (!trimmed.startsWith("```") || !trimmed.endsWith("```")) {
		return trimmed;
	}
	const firstNewline = trimmed.indexOf("\n");
	if (firstNewline < 0) {
		return trimmed;
	}
	return trimmed.slice(firstNewline + 1, -3).trim();
}

function tryParseJsonPlan(raw: string): StructuredPlan | null {
	const trimmed = raw.trim();
	if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
		return null;
	}
	try {
		const parsed = JSON.parse(trimmed) as unknown;
		if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
			return null;
		}
		const candidate = parsed as StructuredPlan;
		if (!candidate.summary && !candidate.steps && !candidate.title) {
			return null;
		}
		return candidate;
	} catch {
		return null;
	}
}
