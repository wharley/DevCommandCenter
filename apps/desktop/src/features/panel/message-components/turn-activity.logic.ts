import type { ModelTokenUsage } from "@dcc/contracts";
import type { AssistantActivityAnnotation } from "./assistant-activity-disclosure";

type ToolCallAnnotation = Extract<AssistantActivityAnnotation, { type: "tool-call" }>;

/** What a tool call did, coarse enough to summarise a turn in one sentence. */
export type ToolKind =
	| "command"
	| "edit"
	| "read"
	| "search"
	| "web"
	| "plan"
	| "agent"
	| "mcp"
	| "other";

const KIND_BY_ACTION: Record<string, ToolKind> = {
	bash: "command",
	shell: "command",
	commandexecution: "command",
	execcommand: "command",
	terminal: "command",
	killshell: "command",
	bashoutput: "command",
	edit: "edit",
	multiedit: "edit",
	write: "edit",
	writefile: "edit",
	applypatch: "edit",
	filechange: "edit",
	notebookedit: "edit",
	read: "read",
	readfile: "read",
	view: "read",
	notebookread: "read",
	grep: "search",
	glob: "search",
	ls: "search",
	searchfiles: "search",
	find: "search",
	codesearch: "search",
	websearch: "web",
	webfetch: "web",
	browser: "web",
	todowrite: "plan",
	updateplan: "plan",
	task: "agent",
	agent: "agent",
};

export function classifyToolAction(action: string): ToolKind {
	if (action.startsWith("mcp__")) return "mcp";
	const normalized = action.toLowerCase().replace(/[^a-z0-9]/g, "");
	return KIND_BY_ACTION[normalized] ?? "other";
}

function shortPath(path: string) {
	const segments = path.split("/").filter(Boolean);
	return segments.length <= 2 ? path : segments.slice(-2).join("/");
}

/** Readable name of an MCP tool: `mcp__github__create_issue` → `github · create_issue`. */
export function mcpToolLabel(action: string) {
	const [, server, ...rest] = action.split("__");
	return rest.length ? `${server} · ${rest.join("__")}` : (server ?? action);
}

/**
 * The concrete thing a call acted on, for the row title: the command line, the
 * file, the search pattern. Kept short; the full value is in the detail view.
 */
export function toolTarget(annotation: ToolCallAnnotation): string | null {
	const command = annotation.detail?.command ?? annotation.command;
	const file = annotation.detail?.file ?? annotation.file;
	const kind = classifyToolAction(annotation.action);
	if (kind === "command" && command) return firstLine(command, 120);
	if ((kind === "edit" || kind === "read") && file) return shortPath(file);
	if (command) return firstLine(command, 120);
	if (file) return shortPath(file);
	return null;
}

function firstLine(value: string, max: number) {
	const line = value.trim().split("\n")[0] ?? "";
	return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

export type DiffStats = { additions: number; deletions: number };

export function diffStats(diff: string | null | undefined): DiffStats | null {
	if (!diff) return null;
	let additions = 0;
	let deletions = 0;
	for (const line of diff.split("\n")) {
		if (line.startsWith("+++") || line.startsWith("---")) continue;
		if (line.startsWith("+")) additions += 1;
		else if (line.startsWith("-")) deletions += 1;
	}
	return additions || deletions ? { additions, deletions } : null;
}

/**
 * A turn rendered in execution order: prose the agent wrote between bursts of
 * work, and the bursts themselves (tools and thinking) as one run each.
 */
export type TurnSegment =
	| { type: "prose"; key: string; annotation: Extract<AssistantActivityAnnotation, { type: "commentary" }> }
	| { type: "work"; key: string; items: Exclude<AssistantActivityAnnotation, { type: "commentary" }>[] };

export function segmentTurn(annotations: readonly AssistantActivityAnnotation[]): TurnSegment[] {
	const segments: TurnSegment[] = [];
	for (const annotation of annotations) {
		if (annotation.type === "commentary") {
			if (!annotation.content.trim()) continue;
			segments.push({ type: "prose", key: annotation.id, annotation });
			continue;
		}
		const last = segments.at(-1);
		if (last?.type === "work") {
			last.items.push(annotation);
		} else {
			segments.push({ type: "work", key: `work-${annotation.id}`, items: [annotation] });
		}
	}
	return segments;
}

export type TurnWorkSummary = {
	counts: Partial<Record<ToolKind, number>>;
	editedFiles: number;
	failures: number;
	thoughts: number;
	tools: number;
};

export function summarizeTurnWork(
	annotations: readonly AssistantActivityAnnotation[],
): TurnWorkSummary {
	const counts: Partial<Record<ToolKind, number>> = {};
	const edited = new Set<string>();
	let failures = 0;
	let thoughts = 0;
	let tools = 0;
	for (const annotation of annotations) {
		if (annotation.type === "reasoning") {
			thoughts += 1;
			continue;
		}
		if (annotation.type !== "tool-call") continue;
		tools += 1;
		if (annotation.status?.type === "failed") failures += 1;
		const kind = classifyToolAction(annotation.action);
		counts[kind] = (counts[kind] ?? 0) + 1;
		const file = annotation.detail?.file ?? annotation.file;
		if (kind === "edit") edited.add(file ?? annotation.id);
	}
	return { counts, editedFiles: edited.size, failures, thoughts, tools };
}

/** Order in which kinds appear in the one-line turn summary. */
export const SUMMARY_KIND_ORDER: readonly ToolKind[] = [
	"edit",
	"command",
	"read",
	"search",
	"web",
	"mcp",
	"agent",
	"plan",
	"other",
];

export function formatElapsed(ms: number) {
	const totalSeconds = Math.max(0, Math.round(ms / 1000));
	if (totalSeconds < 60) return `${totalSeconds}s`;
	const hours = Math.floor(totalSeconds / 3600);
	const minutes = Math.floor((totalSeconds % 3600) / 60);
	const seconds = totalSeconds % 60;
	if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
	return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

export function turnDurationMs(
	startedAt: string | null | undefined,
	endedAt: string | null | undefined,
): number | null {
	if (!startedAt || !endedAt) return null;
	const start = Date.parse(startedAt);
	const end = Date.parse(endedAt);
	if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return null;
	return end - start;
}

export type TurnTokenTotals = {
	input: number;
	output: number;
	cached: number;
	reasoning: number;
	total: number;
	costUsd: number | null;
};

export function totalTurnTokens(models: readonly ModelTokenUsage[] | undefined): TurnTokenTotals | null {
	if (!models?.length) return null;
	const totals: TurnTokenTotals = {
		input: 0,
		output: 0,
		cached: 0,
		reasoning: 0,
		total: 0,
		costUsd: null,
	};
	for (const usage of models) {
		totals.input += usage.inputTokens;
		totals.output += usage.outputTokens;
		totals.cached += usage.cachedInputTokens;
		totals.reasoning += usage.reasoningOutputTokens;
		totals.total += usage.totalTokens || usage.inputTokens + usage.outputTokens;
		if (typeof usage.costUsd === "number") {
			totals.costUsd = (totals.costUsd ?? 0) + usage.costUsd;
		}
	}
	return totals.total > 0 ? totals : null;
}

export function formatTokenCount(value: number) {
	if (value < 1000) return String(value);
	if (value < 1_000_000) return `${(value / 1000).toFixed(value < 10_000 ? 1 : 0)}k`;
	return `${(value / 1_000_000).toFixed(1)}M`;
}

/** Removes ANSI escape sequences that CLIs leave in captured output. */
export function stripAnsi(value: string) {
	// eslint-disable-next-line no-control-regex
	return value.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]|\u001b\][^\u0007]*\u0007/g, "");
}

export type DiffLineKind = "add" | "del" | "hunk" | "meta" | "context";

export function classifyDiffLine(line: string): DiffLineKind {
	if (line.startsWith("+++") || line.startsWith("---")) return "meta";
	if (line.startsWith("@@")) return "hunk";
	if (line.startsWith("+")) return "add";
	if (line.startsWith("-")) return "del";
	return "context";
}
