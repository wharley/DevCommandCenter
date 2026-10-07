import type {
	ExecuteConversationRewindInput,
	ExecuteConversationRewindOutput,
	PrepareConversationRewindOutput,
} from "@dcc/contracts";
import {
	resolveGuardedUndoCapture,
	resolveGuardedUndoFailureReason,
} from "@/features/panel/turn-review.logic";

/**
 * "Edit from here": the person picks one of their messages; the conversation
 * goes back to just before it and the message returns to the composer. The
 * backend decides everything up front (which turns leave, how the provider
 * follows, whether files can be restored); this module only turns that plan
 * into what the dialog offers and what the outcome means.
 */
export type EditFromHerePlan = Extract<PrepareConversationRewindOutput, { status: "ready" }>;

export type EditFromHereContinuation =
	/** This thread; the provider's own conversation is cut at the same point. */
	| { kind: "native" }
	/** This thread; nothing is kept, so the provider starts over. */
	| { kind: "fresh" }
	/** A new thread re-anchored on what came before; this one stays as is. */
	| { kind: "new_thread"; reasonKey: string };

export function editFromHereContinuation(plan: EditFromHerePlan): EditFromHereContinuation {
	switch (plan.provider.mode) {
		case "native":
			return { kind: "native" };
		case "fresh":
			return { kind: "fresh" };
		case "new_thread":
			return {
				kind: "new_thread",
				reasonKey: `conversation.editFromHere.newThreadReasons.${plan.provider.reason}`,
			};
	}
}

export type EditFromHereFiles =
	| { kind: "none" }
	| { kind: "restorable"; fileCount: number; turnCount: number }
	/** `reasonKey` reuses the Guarded Undo explanations. */
	| { kind: "blocked"; reasonKey: string };

export function editFromHereFiles(plan: EditFromHerePlan): EditFromHereFiles {
	const files = plan.files;
	switch (files.status) {
		case "nothing_to_restore":
			return { kind: "none" };
		case "restorable":
			return { kind: "restorable", fileCount: files.fileCount, turnCount: files.turnCount };
		case "not_restorable":
			return {
				kind: "blocked",
				reasonKey:
					files.stage === "capture"
						? `turnReview.guardedUndo.reasons.${
								resolveGuardedUndoCapture({
									state: "ineligible",
									reasonCode: files.reasonCode,
									fileCount: 0,
									artifactBytes: 0,
									completedAt: null,
									expiresAt: null,
								}).reason
							}`
						: `turnReview.guardedUndo.failureReasons.${resolveGuardedUndoFailureReason(
								files.reasonCode,
							)}`,
			};
	}
}

/**
 * The confirmations the dialog offers. Restoring files is never implied:
 * it is its own button, offered only when every changed file can be restored.
 */
export function editFromHereActions(plan: EditFromHerePlan): Array<"restore" | "keep"> {
	return editFromHereFiles(plan).kind === "restorable" ? ["restore", "keep"] : ["keep"];
}

export function buildExecuteConversationRewindInput(
	sessionId: string,
	plan: EditFromHerePlan,
	restoreFiles: boolean,
): ExecuteConversationRewindInput {
	return {
		sessionId,
		anchorTurnId: plan.anchorTurnId,
		removedTurnIds: plan.removedTurnIds,
		restoreFiles: restoreFiles && editFromHereFiles(plan).kind === "restorable",
		continueInNewThread: plan.provider.mode === "new_thread",
	};
}

export type EditFromHereOutcome =
	| {
			kind: "rewound";
			prompt: string;
			restoredTurnCount: number;
			continuation: "native" | "fresh";
	  }
	/** Files are settled; fork the new thread with this prompt in its composer. */
	| { kind: "fork"; prompt: string; restoredTurnCount: number }
	/** Restoring stopped; the conversation did not move. */
	| {
			kind: "files_stopped";
			restoredTurnCount: number;
			resultKey: string;
			reasonKey: string | null;
	  }
	| { kind: "refused"; reason: string };

export function interpretConversationRewind(
	output: ExecuteConversationRewindOutput,
): EditFromHereOutcome {
	switch (output.status) {
		case "rewound":
			return {
				kind: "rewound",
				prompt: output.anchorPrompt,
				restoredTurnCount: output.restoredTurnIds.length,
				continuation: output.provider.mode === "fresh" ? "fresh" : "native",
			};
		case "continue_in_new_thread":
			return {
				kind: "fork",
				prompt: output.anchorPrompt,
				restoredTurnCount: output.restoredTurnIds.length,
			};
		case "files_stopped": {
			const { stopped } = output;
			const outcome = ["blocked", "rolled_back", "recovery_required"].includes(stopped.outcome)
				? stopped.outcome
				: "blocked";
			return {
				kind: "files_stopped",
				restoredTurnCount: stopped.restoredTurnIds.length,
				resultKey: `turnReview.guardedUndo.results.${outcome}`,
				reasonKey: stopped.reasonCode
					? `turnReview.guardedUndo.failureReasons.${resolveGuardedUndoFailureReason(
							stopped.reasonCode,
						)}`
					: null,
			};
		}
		case "refused":
			return { kind: "refused", reason: output.reason };
	}
}
