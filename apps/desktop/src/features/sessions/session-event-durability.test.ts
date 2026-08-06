import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import {
	isTerminalEventDurable,
	terminalDurabilityTarget,
} from "./session-event-durability";

const event = (value: object) => value as CoreEvent;
const record = (value: object) => value as SessionEventRecord;

describe("session event durability barrier", () => {
	it.each([
		["sessionTurnCompleted", "turn_completed"],
		["sessionTurnAborted", "turn_aborted"],
	] as const)("matches durable %s by session and turn", (eventType, recordType) => {
		const target = terminalDurabilityTarget(
			event({ [eventType]: { session_id: "s", turn_id: "t" } }),
		);
		expect(target).not.toBeNull();
		expect(
			isTerminalEventDurable(
				target!,
				[record({ sessionId: "s", kind: { type: recordType, turnId: "t" } })],
			),
		).toBe(true);
		expect(
			isTerminalEventDurable(
				target!,
				[record({ sessionId: "s", kind: { type: recordType, turnId: "other" } })],
			),
		).toBe(false);
	});

	it.each([
		["sessionAborted", "session_aborted"],
		["sessionCompleted", "session_completed"],
	] as const)("requires a persisted %s record", (eventType, recordType) => {
		const target = terminalDurabilityTarget(
			event({ [eventType]: { session_id: "s", reason: "stop" } }),
		);
		expect(
			isTerminalEventDurable(
				target!,
				[record({ sessionId: "s", kind: { type: recordType } })],
			),
		).toBe(true);
		expect(isTerminalEventDurable(target!, [])).toBe(false);
	});
});
