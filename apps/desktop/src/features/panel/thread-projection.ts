import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import {
	projectWorkspaceMessages,
	type WorkspaceMessage,
} from "@/features/sessions/session-thread-history.logic";

function isHeavyActivityEvent(event: CoreEvent) {
	return (
		"sessionTurnReasoningStarted" in event ||
		"sessionTurnReasoningDelta" in event ||
		"sessionTurnReasoningCompleted" in event ||
		"sessionTurnToolCallStarted" in event ||
		"sessionTurnToolCallDelta" in event ||
		"sessionTurnToolCallCompleted" in event ||
		"sessionTurnToolCallFailed" in event
	);
}

/** Inspector plan state does not need to materialize tool/reasoning transcripts. */
export function projectWorkspacePlanMessages(
	history: SessionEventRecord[],
	liveEvents: CoreEvent[],
	sessionId: string | null,
): WorkspaceMessage[] {
	return projectWorkspaceMessages(
		history.filter((record) =>
			!record.kind.type.startsWith("turn_reasoning_") &&
			!record.kind.type.startsWith("turn_tool_call_"),
		),
		liveEvents.filter((event) => !isHeavyActivityEvent(event)),
		sessionId,
		null,
	);
}

export {
	projectWorkspaceMessages,
	type SessionMessageStatus,
	type WorkspaceMessage,
	type WorkspaceMessageAnnotation,
	type WorkspaceMessageRole,
} from "@/features/sessions/session-thread-history.logic";
