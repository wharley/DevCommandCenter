import { classifyToolAction } from "@/features/panel/message-components/turn-activity.logic";
import type { WorkspaceMessage } from "./session-thread-history.logic";

/** A build/test/lint command the parent agent ran after a delegation came back. */
export type DelegationCheck = { command: string; ok: boolean };

const VALIDATION_WORDS = new Set([
	"test",
	"tests",
	"build",
	"check",
	"lint",
	"typecheck",
	"tsc",
	"vitest",
	"jest",
	"pytest",
	"xcodebuild",
	"xctest",
	"clippy",
]);
const MAX_CHECKS = 4;
const MAX_COMMAND_CHARS = 48;

/** The segment of a shell line that does the validating: `cd x && swift test | tail` → `swift test`. */
export function validationSegment(command: string): string | null {
	const firstLine = command.split("\n", 1)[0] ?? "";
	for (const segment of firstLine.split(/&&|\|\||;|\|/)) {
		const trimmed = segment.trim();
		const words = trimmed.toLowerCase().split(/[^a-z0-9]+/);
		if (words.some((word) => VALIDATION_WORDS.has(word))) {
			const clean = trimmed.replace(/\s+\d?>.*$/, "").trim();
			return clean.length > MAX_COMMAND_CHARS
				? `${clean.slice(0, MAX_COMMAND_CHARS - 1).trimEnd()}…`
				: clean;
		}
	}
	return null;
}

/**
 * What the parent agent verified for each finished delegation: the validation
 * commands of the turn the `[DCC]` hand-back started, once that turn settled.
 * Deterministic — read from the recorded tool calls, never from the reply text.
 */
export function delegationVerifications(
	messages: readonly WorkspaceMessage[],
): Map<string, DelegationCheck[]> {
	const delegationByTurn = new Map<string, string>();
	for (const message of messages) {
		if (
			message.role === "user" &&
			message.turnId &&
			message.delegationHandBack?.outcome === "finished"
		) {
			delegationByTurn.set(message.turnId, message.delegationHandBack.delegationId);
		}
	}
	const result = new Map<string, DelegationCheck[]>();
	if (delegationByTurn.size === 0) return result;
	for (const message of messages) {
		const delegationId = message.turnId ? delegationByTurn.get(message.turnId) : undefined;
		if (!delegationId || message.role !== "assistant" || !message.turnSettled) continue;
		const checks = new Map<string, boolean>();
		for (const annotation of message.annotations ?? []) {
			if (annotation.type !== "tool-call" || classifyToolAction(annotation.action) !== "command") {
				continue;
			}
			const segment = validationSegment(annotation.detail?.command ?? annotation.command ?? "");
			if (!segment) continue;
			const exitCode = annotation.detail?.exitCode;
			// The latest run of a command wins: a fix-and-rerun ends green.
			checks.delete(segment);
			checks.set(
				segment,
				annotation.status?.type !== "failed" && (exitCode == null || exitCode === 0),
			);
		}
		if (checks.size > 0) {
			result.set(
				delegationId,
				[...checks].slice(-MAX_CHECKS).map(([command, ok]) => ({ command, ok })),
			);
		}
	}
	return result;
}
