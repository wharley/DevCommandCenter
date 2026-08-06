import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";

export type TerminalDurabilityTarget =
	| { sessionId: string; type: "turn_completed" | "turn_aborted"; turnId: string }
	| { sessionId: string; type: "session_completed" }
	| { sessionId: string; type: "session_aborted" };

export function terminalDurabilityTarget(
	event: CoreEvent,
): TerminalDurabilityTarget | null {
	if ("sessionTurnCompleted" in event && event.sessionTurnCompleted) {
		return {
			sessionId: event.sessionTurnCompleted.session_id,
			type: "turn_completed",
			turnId: event.sessionTurnCompleted.turn_id,
		};
	}
	if ("sessionTurnAborted" in event && event.sessionTurnAborted) {
		return {
			sessionId: event.sessionTurnAborted.session_id,
			type: "turn_aborted",
			turnId: event.sessionTurnAborted.turn_id,
		};
	}
	if ("sessionAborted" in event && event.sessionAborted) {
		return {
			sessionId: event.sessionAborted.session_id,
			type: "session_aborted",
		};
	}
	if ("sessionCompleted" in event && event.sessionCompleted) {
		return {
			sessionId: event.sessionCompleted.session_id,
			type: "session_completed",
		};
	}
	return null;
}

export function isTerminalEventDurable(
	target: TerminalDurabilityTarget,
	history: SessionEventRecord[],
) {
	return history.some((record) => {
		if (record.sessionId !== target.sessionId || record.kind.type !== target.type) {
			return false;
		}
		if (!("turnId" in target)) return true;
		return "turnId" in record.kind && record.kind.turnId === target.turnId;
	});
}
