import type { Delegation, DelegationMode, ProviderCatalog } from "@dcc/contracts";
import { describeDelegation } from "./delegation-decisions";

/**
 * The message that hands a finished delegation back to the agent that asked
 * for it. Providers keep their own native sessions, so a delegation result
 * only reaches the parent's context as a turn. The text is a deterministic
 * template over what DCC recorded — never generated.
 */
export type DelegationResultTurnInput = {
	mode: DelegationMode;
	providerLabel: string;
	modelLabel?: string | null;
	instruction: string;
	status: "completed" | "review_pending" | "failed";
	summary?: string | null;
	touchedFiles?: readonly string[];
	validationSummary?: string | null;
	failureReason?: string | null;
};

const MAX_INSTRUCTION_CHARS = 600;
const MAX_SUMMARY_CHARS = 4_000;
const MAX_VALIDATION_CHARS = 1_200;
const MAX_LISTED_FILES = 20;

function clip(value: string, maxLength: number) {
	const trimmed = value.trim();
	return trimmed.length <= maxLength
		? trimmed
		: `${trimmed.slice(0, maxLength).trimEnd()}… [truncated]`;
}

export function buildDelegationResultTurn(input: DelegationResultTurnInput): string {
	const agent = input.modelLabel
		? `${input.providerLabel} (${input.modelLabel})`
		: input.providerLabel;
	const outcome = input.status === "failed" ? "failed" : "finished";
	const lines = [
		`[DCC] Delegated ${input.mode} task ${outcome} — ${agent}.`,
		"",
		"Task you delegated:",
		clip(input.instruction, MAX_INSTRUCTION_CHARS),
	];

	if (input.status === "failed") {
		lines.push(
			"",
			`Failure: ${clip(input.failureReason?.trim() || "no reason recorded", MAX_SUMMARY_CHARS)}`,
			"",
			"Decide whether to continue without this result or ask the human how to proceed.",
		);
		return lines.join("\n");
	}

	lines.push(
		"",
		"Result:",
		clip(input.summary?.trim() || "The delegated agent returned no text.", MAX_SUMMARY_CHARS),
	);

	const files = input.touchedFiles ?? [];
	if (files.length > 0) {
		lines.push(
			"",
			`Files touched (${files.length}):`,
			...files.slice(0, MAX_LISTED_FILES).map((file) => `- ${file}`),
			...(files.length > MAX_LISTED_FILES
				? [`- … ${files.length - MAX_LISTED_FILES} more`]
				: []),
		);
	}
	if (input.status === "review_pending") {
		lines.push(
			"",
			"These edits live in an isolated delegation worktree and are NOT applied to your workspace yet. The human reviews and applies or discards them in the DCC Inspector — do not re-implement them.",
		);
	}

	const validation = input.validationSummary?.trim();
	if (validation) {
		lines.push("", "Validation:", clip(validation, MAX_VALIDATION_CHARS));
	}

	lines.push("", "Continue your task using this result.");
	return lines.join("\n");
}

/** The instruction block of a prompt built by `buildManualDelegationPrompt`. */
export function extractDelegationInstruction(prompt: string): string {
	const marker = "\nInstruction:\n";
	const start = prompt.indexOf(marker);
	if (start < 0) {
		return prompt.trim();
	}
	const rest = prompt.slice(start + marker.length);
	const end = rest.search(/\n\n(?:Git context|Mission spec|Recent parent session context):\n/);
	return (end < 0 ? rest : rest.slice(0, end)).trim();
}

/** Builds the hand-back turn from a stored delegation record, when it has finished. */
export function delegationResultTurnFromRecord(
	record: Delegation,
	providers: ProviderCatalog["providers"],
	/** The failure reason lives on the timeline event, not on the record. */
	failureReason?: string | null,
): string | null {
	if (
		record.status !== "completed" &&
		record.status !== "review_pending" &&
		record.status !== "failed"
	) {
		return null;
	}
	const decisions = describeDelegation(record, providers);
	return buildDelegationResultTurn({
		mode: record.mode,
		providerLabel: decisions.providerLabel,
		modelLabel: decisions.modelLabel,
		instruction: extractDelegationInstruction(record.prompt),
		status: record.status,
		summary: record.resultSummary,
		touchedFiles: record.touchedFiles ?? [],
		validationSummary: record.validationSummary,
		failureReason: record.status === "failed" ? (failureReason ?? record.resultSummary) : null,
	});
}
