import { describe, expect, it } from "vitest";
import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import {
	isPlanVersionApproved,
	isPlanVersionHandedOff,
} from "./plan-approval";

const identity = {
	sessionId: "session-1",
	planMessageId: "assistant-session-1-turn-1",
	planVersion: 1,
	planHash: "fnv1a32:12345678",
};

function persistedApproval(): SessionEventRecord {
	return {
		eventId: "event-plan-approved",
		sessionId: "session-1",
		sequence: 5,
		occurredAt: "2026-07-23T10:00:00Z",
		kind: {
			type: "plan_approved",
			planMessageId: identity.planMessageId,
			planVersion: identity.planVersion,
			planHash: identity.planHash,
		},
	};
}

function liveApproval(): CoreEvent {
	return {
		sessionPlanApproved: {
			session_id: identity.sessionId,
			plan_message_id: identity.planMessageId,
			plan_version: identity.planVersion,
			plan_hash: identity.planHash,
		},
	};
}

function persistedHandoff(): SessionEventRecord {
	return {
		eventId: "event-plan-handed-off",
		sessionId: "session-1",
		sequence: 6,
		occurredAt: "2026-07-23T10:01:00Z",
		kind: {
			type: "plan_handed_off",
			planMessageId: identity.planMessageId,
			planVersion: identity.planVersion,
			planHash: identity.planHash,
			action: "delegation",
			targetSessionId: null,
		},
	};
}

describe("plan approval persistence", () => {
	it("restores approval from persisted history after a restart", () => {
		expect(isPlanVersionApproved(identity, [persistedApproval()], [])).toBe(true);
	});

	it("accepts the matching live event before history refetch completes", () => {
		expect(isPlanVersionApproved(identity, [], [liveApproval()])).toBe(true);
	});

	it("does not carry approval to a revised plan", () => {
		expect(
			isPlanVersionApproved(
				{
					...identity,
					planMessageId: "assistant-session-1-turn-2",
					planVersion: 2,
					planHash: "fnv1a32:87654321",
				},
				[persistedApproval()],
				[],
			),
		).toBe(false);
	});

	it("invalidates approval when the exact plan content changes", () => {
		expect(
			isPlanVersionApproved(
				{ ...identity, planHash: "fnv1a32:aaaaaaaa" },
				[persistedApproval()],
				[],
			),
		).toBe(false);
	});

	it("restores the read-only handoff state from persisted history", () => {
		expect(
			isPlanVersionHandedOff(identity, [persistedHandoff()], []),
		).toBe(true);
	});

	it("does not carry the handoff lock to a revised plan", () => {
		expect(
			isPlanVersionHandedOff(
				{
					...identity,
					planMessageId: "assistant-session-1-turn-2",
					planVersion: 2,
					planHash: "fnv1a32:87654321",
				},
				[persistedHandoff()],
				[],
			),
		).toBe(false);
	});
});
