import type { CoreEvent } from "@dcc/contracts";

/**
 * Keeps the default activity view focused on user-meaningful milestones.
 * Streaming deltas and successful tool-call internals remain available in the
 * diagnostics view without flooding the daily timeline.
 */
export function isSemanticSessionEvent(event: CoreEvent): boolean {
	return !(
		"sessionTurnDelta" in event ||
		"sessionTurnReasoningStarted" in event ||
		"sessionTurnReasoningDelta" in event ||
		"sessionTurnReasoningCompleted" in event ||
		"sessionMcpRuntimeStatusChanged" in event ||
		"sessionTurnToolCallStarted" in event ||
		"sessionTurnToolCallDelta" in event ||
		"sessionTurnToolCallCompleted" in event
	);
}
