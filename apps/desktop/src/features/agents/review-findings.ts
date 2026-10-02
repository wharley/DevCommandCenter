import type { DiffMachineAnnotation } from "@/features/editor/diff-types";

export type ReviewSeverity = "critical" | "major" | "minor";

/** One problem the reviewer verified, anchored to the current file content. */
export type ReviewFinding = {
	path: string;
	startLine: number;
	endLine: number;
	severity: ReviewSeverity;
	title: string;
	/** The full explanation: what is wrong, the failure scenario and what to change. */
	detail: string;
};

const MAX_FINDINGS = 50;
const MAX_TITLE_CHARS = 240;
const MAX_DETAIL_CHARS = 1_500;
// The block ends at a fence on a line of its own, so a code fence quoted inside
// a finding's text does not cut the JSON short. The closing fence is optional:
// a block still being streamed is recognized and hidden.
const REVIEW_BLOCK = /```dcc-review[^\n]*\n?([\s\S]*?)(?:\n```[ \t]*(?=\n|$)|$)/;

function positiveLine(value: unknown): number | null {
	return typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null;
}

/**
 * Reads the machine-readable block the built-in reviewer ends its message
 * with. Returns `null` when the message has no block or the block is not
 * valid, so a review without one is never mistaken for "no findings".
 * Entries that cannot be placed on a line are dropped.
 */
export function parseReviewFindings(message: string): ReviewFinding[] | null {
	const body = message.match(REVIEW_BLOCK)?.[1];
	if (body === undefined) {
		return null;
	}
	let parsed: unknown;
	try {
		parsed = JSON.parse(body);
	} catch {
		return null;
	}
	const list = (parsed as { findings?: unknown } | null)?.findings;
	if (!Array.isArray(list)) {
		return null;
	}
	return list.slice(0, MAX_FINDINGS).flatMap((value): ReviewFinding[] => {
		if (!value || typeof value !== "object") {
			return [];
		}
		const candidate = value as Record<string, unknown>;
		const path = typeof candidate.path === "string" ? candidate.path.trim().replace(/^\.\//, "") : "";
		const startLine = positiveLine(candidate.line);
		const title = typeof candidate.title === "string" ? candidate.title.trim() : "";
		if (!path || !startLine || !title) {
			return [];
		}
		const endLine = positiveLine(candidate.endLine);
		return [
			{
				path,
				startLine,
				endLine: endLine && endLine >= startLine ? endLine : startLine,
				severity:
					candidate.severity === "critical" || candidate.severity === "major"
						? candidate.severity
						: "minor",
				title: title.length > MAX_TITLE_CHARS ? `${title.slice(0, MAX_TITLE_CHARS)}…` : title,
				detail:
					typeof candidate.detail === "string"
						? candidate.detail.trim().slice(0, MAX_DETAIL_CHARS)
						: "",
			},
		];
	});
}

/** The message as the person should read it: without the machine block. */
export function stripReviewBlock(message: string): string {
	return message.replace(REVIEW_BLOCK, "").trimEnd();
}

/** Gutter markers for one file of the diff. Findings point at the new side. */
export function reviewAnnotationsForPath(
	path: string,
	findings: ReviewFinding[],
): DiffMachineAnnotation[] {
	return findings
		.filter((finding) => finding.path === path)
		.map((finding) => ({
			source: "agent-review" as const,
			severity: finding.severity,
			side: "modified" as const,
			startLine: finding.startLine,
			endLine: finding.endLine,
			title: finding.title,
			detail: finding.detail,
		}));
}
