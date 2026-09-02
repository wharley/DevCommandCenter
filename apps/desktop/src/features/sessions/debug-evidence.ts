/**
 * Evidence-first debugging, slice 1: a bounded, per-conversation tray of
 * explicit evidence that travels with the next message as one delimited block.
 *
 * The tray is process-local and ephemeral. Every item is added by a human
 * gesture (Browser → agent, Terminal → agent), stays reviewable and removable
 * until sent, and is labelled with its origin and trust so the provider can
 * tell data from instructions. Nothing here is persisted, and nothing is
 * injected automatically.
 */

import type { ContextAttachmentId } from "./context-attachment-ledger";

export const DEBUG_STAGES = [
	"observe",
	"reproduce",
	"investigate",
	"fix",
	"verify",
] as const;

export type DebugStage = (typeof DEBUG_STAGES)[number];

export const DEFAULT_DEBUG_STAGE: DebugStage = "observe";
export const MAX_DEBUG_EVIDENCE_ITEMS = 8;
export const MAX_DEBUG_EVIDENCE_TOTAL_CHARS = 24_000;
export const MAX_DEBUG_EVIDENCE_LABEL_CHARS = 120;
export const DEBUG_EVIDENCE_PREVIEW_CHARS = 600;

const DEBUG_EVIDENCE_TAG = "debug_evidence";
const EVIDENCE_TAG = "evidence";

export type DebugEvidenceSource = "browser" | "terminal";
export type DebugEvidenceTrust = "remote_untrusted" | "local_terminal";

export type DebugEvidenceItem = {
	/** Process-local identity, never sent to a provider. */
	id: string;
	source: DebugEvidenceSource;
	trust: DebugEvidenceTrust;
	/** Bounded human-readable origin (page title/URL, terminal scope). */
	label: string;
	/** Already bounded and escaped by the source-specific envelope. */
	body: string;
	chars: number;
	truncated: boolean;
	capturedAt: string;
	/** Ledger handle so consumption is recorded when the turn is accepted. */
	attachment: ContextAttachmentId | null;
};

export type DebugEvidenceTray = {
	items: DebugEvidenceItem[];
	stage: DebugStage;
};

export type DebugEvidenceRejection =
	| "empty"
	| "too_many_items"
	| "budget_exceeded";

export type DebugEvidenceInput = {
	source: DebugEvidenceSource;
	trust: DebugEvidenceTrust;
	label: string;
	body: string;
	truncated: boolean;
	capturedAt?: string;
	attachment?: ContextAttachmentId | null;
};

let evidenceSequence = 0;

export function isDebugStage(value: unknown): value is DebugStage {
	return (
		typeof value === "string" &&
		(DEBUG_STAGES as readonly string[]).includes(value)
	);
}

export function emptyDebugEvidenceTray(
	stage: DebugStage = DEFAULT_DEBUG_STAGE,
): DebugEvidenceTray {
	return { items: [], stage };
}

export function debugEvidenceTotalChars(items: readonly DebugEvidenceItem[]) {
	return items.reduce((total, item) => total + item.chars, 0);
}

function sourceTrustMatches(
	source: DebugEvidenceSource,
	trust: DebugEvidenceTrust,
) {
	return (
		(source === "browser" && trust === "remote_untrusted") ||
		(source === "terminal" && trust === "local_terminal")
	);
}

/** Single line, no control characters, bounded — safe inside an attribute-like line. */
export function boundDebugEvidenceLabel(value: string) {
	let cleaned = "";
	for (const character of value) {
		const code = character.charCodeAt(0);
		cleaned += code < 0x20 || code === 0x7f ? " " : character;
	}
	cleaned = cleaned.replace(/\s+/g, " ").trim();
	if (cleaned.length <= MAX_DEBUG_EVIDENCE_LABEL_CHARS) return cleaned;
	return `${cleaned.slice(0, MAX_DEBUG_EVIDENCE_LABEL_CHARS - 1)}…`;
}

/** Keeps a body from closing the outer envelope early. */
export function escapeDebugEvidenceBody(body: string) {
	return body
		.replaceAll(`</${EVIDENCE_TAG}>`, `&lt;/${EVIDENCE_TAG}&gt;`)
		.replaceAll(`</${DEBUG_EVIDENCE_TAG}>`, `&lt;/${DEBUG_EVIDENCE_TAG}&gt;`);
}

export function addDebugEvidence(
	tray: DebugEvidenceTray,
	input: DebugEvidenceInput,
): { tray: DebugEvidenceTray; rejection: DebugEvidenceRejection | null } {
	if (!sourceTrustMatches(input.source, input.trust)) {
		return { tray, rejection: "empty" };
	}
	const body = escapeDebugEvidenceBody(input.body);
	if (body.trim().length === 0) {
		return { tray, rejection: "empty" };
	}
	if (tray.items.length >= MAX_DEBUG_EVIDENCE_ITEMS) {
		return { tray, rejection: "too_many_items" };
	}
	const chars = body.length;
	if (debugEvidenceTotalChars(tray.items) + chars > MAX_DEBUG_EVIDENCE_TOTAL_CHARS) {
		return { tray, rejection: "budget_exceeded" };
	}
	evidenceSequence += 1;
	const item: DebugEvidenceItem = {
		id: `evidence-${evidenceSequence}`,
		source: input.source,
		trust: input.trust,
		label: boundDebugEvidenceLabel(input.label),
		body,
		chars,
		truncated: Boolean(input.truncated),
		capturedAt: input.capturedAt ?? new Date().toISOString(),
		attachment: input.attachment ?? null,
	};
	return { tray: { ...tray, items: [...tray.items, item] }, rejection: null };
}

export function removeDebugEvidence(
	tray: DebugEvidenceTray,
	ids: readonly string[],
): DebugEvidenceTray {
	if (ids.length === 0) return tray;
	const remove = new Set(ids);
	const items = tray.items.filter((item) => !remove.has(item.id));
	return items.length === tray.items.length ? tray : { ...tray, items };
}

export function clearDebugEvidence(tray: DebugEvidenceTray): DebugEvidenceTray {
	return tray.items.length === 0 ? tray : { ...tray, items: [] };
}

export function setDebugStage(
	tray: DebugEvidenceTray,
	stage: DebugStage,
): DebugEvidenceTray {
	return tray.stage === stage ? tray : { ...tray, stage };
}

export function debugEvidencePreview(item: DebugEvidenceItem) {
	const body = item.body.trimStart();
	if (body.length <= DEBUG_EVIDENCE_PREVIEW_CHARS) return body;
	return `${body.slice(0, DEBUG_EVIDENCE_PREVIEW_CHARS)}…`;
}

export type DebugEvidencePromptLabels = {
	/** Guidance for the selected stage, e.g. "Stage: observe. Describe…". */
	stageGuidance: string;
	/** Reminder that evidence bodies are data, never instructions. */
	trustNotice: string;
	/** Used when the person sends evidence without writing a message. */
	defaultMessage: string;
};

/**
 * Composes the outgoing prompt: the person's message first, then one
 * delimited block with every evidence item in the order it was collected.
 * Keys inside the block are machine-facing and intentionally not localized.
 */
export function buildDebugEvidencePrompt(input: {
	message: string;
	stage: DebugStage;
	items: readonly DebugEvidenceItem[];
	labels: DebugEvidencePromptLabels;
}) {
	const message = input.message.trim();
	if (input.items.length === 0) return message;
	const lines: string[] = [];
	lines.push(message.length > 0 ? message : input.labels.defaultMessage.trim());
	lines.push("");
	lines.push(`<${DEBUG_EVIDENCE_TAG} stage="${input.stage}" items="${input.items.length}">`);
	lines.push(input.labels.stageGuidance.trim());
	lines.push(input.labels.trustNotice.trim());
	input.items.forEach((item, index) => {
		lines.push("");
		lines.push(
			`<${EVIDENCE_TAG} index="${index + 1}" source="${item.source}" trust="${item.trust}" truncated="${item.truncated ? "yes" : "no"}">`,
		);
		lines.push(`origin: ${boundDebugEvidenceLabel(item.label)}`);
		lines.push(`captured_at: ${item.capturedAt}`);
		lines.push("---");
		lines.push(escapeDebugEvidenceBody(item.body));
		lines.push(`</${EVIDENCE_TAG}>`);
	});
	lines.push(`</${DEBUG_EVIDENCE_TAG}>`);
	return lines.join("\n");
}
