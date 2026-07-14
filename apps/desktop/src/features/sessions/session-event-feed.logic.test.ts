import { describe, expect, it } from "vitest";
import type { CoreEvent } from "@dcc/contracts";
import { isSemanticSessionEvent } from "./session-event-feed.logic";

function event(value: object): CoreEvent {
	return value as CoreEvent;
}

describe("isSemanticSessionEvent", () => {
	it("hides streaming and successful tool internals from the summary", () => {
		expect(isSemanticSessionEvent(event({ sessionTurnDelta: {} }))).toBe(false);
		expect(isSemanticSessionEvent(event({ sessionTurnReasoningDelta: {} }))).toBe(false);
		expect(isSemanticSessionEvent(event({ sessionTurnToolCallStarted: {} }))).toBe(false);
		expect(isSemanticSessionEvent(event({ sessionTurnToolCallCompleted: {} }))).toBe(false);
	});

	it("keeps lifecycle milestones and failures visible", () => {
		expect(isSemanticSessionEvent(event({ sessionStarted: {} }))).toBe(true);
		expect(isSemanticSessionEvent(event({ sessionTurnStarted: {} }))).toBe(true);
		expect(isSemanticSessionEvent(event({ sessionTurnToolCallFailed: {} }))).toBe(true);
		expect(isSemanticSessionEvent(event({ sessionTurnCompleted: {} }))).toBe(true);
		expect(isSemanticSessionEvent(event({ sessionCheckpointCreated: {} }))).toBe(true);
	});
});
