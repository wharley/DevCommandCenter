import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";

type PlanApprovalIdentity = {
	sessionId: string | null;
	planMessageId: string | null;
	planVersion: number;
	planHash: string | null;
};

export function isPlanVersionApproved(
	identity: PlanApprovalIdentity,
	historyEvents: SessionEventRecord[],
	liveEvents: CoreEvent[],
) {
	if (
		!identity.sessionId ||
		!identity.planMessageId ||
		!identity.planHash ||
		identity.planVersion <= 0
	) {
		return false;
	}

	const matches = (approval: {
		sessionId: string;
		planMessageId: string;
		planVersion: number;
		planHash: string;
	}) =>
		approval.sessionId === identity.sessionId &&
		approval.planMessageId === identity.planMessageId &&
		approval.planVersion === identity.planVersion &&
		approval.planHash === identity.planHash;

	if (
		historyEvents.some(
			(record) =>
				record.kind.type === "plan_approved" &&
				matches({
					sessionId: record.sessionId,
					planMessageId: record.kind.planMessageId,
					planVersion: record.kind.planVersion,
					planHash: record.kind.planHash,
				}),
		)
	) {
		return true;
	}

	return liveEvents.some((event) => {
		const approval =
			"sessionPlanApproved" in event ? event.sessionPlanApproved : null;
		return Boolean(
			approval &&
				matches({
					sessionId: approval.session_id,
					planMessageId: approval.plan_message_id,
					planVersion: approval.plan_version,
					planHash: approval.plan_hash,
				}),
		);
	});
}

export function isPlanVersionHandedOff(
	identity: PlanApprovalIdentity,
	historyEvents: SessionEventRecord[],
	liveEvents: CoreEvent[],
) {
	if (
		!identity.sessionId ||
		!identity.planMessageId ||
		!identity.planHash ||
		identity.planVersion <= 0
	) {
		return false;
	}

	const matches = (handoff: {
		sessionId: string;
		planMessageId: string;
		planVersion: number;
		planHash: string;
	}) =>
		handoff.sessionId === identity.sessionId &&
		handoff.planMessageId === identity.planMessageId &&
		handoff.planVersion === identity.planVersion &&
		handoff.planHash === identity.planHash;

	if (
		historyEvents.some(
			(record) =>
				record.kind.type === "plan_handed_off" &&
				matches({
					sessionId: record.sessionId,
					planMessageId: record.kind.planMessageId,
					planVersion: record.kind.planVersion,
					planHash: record.kind.planHash,
				}),
		)
	) {
		return true;
	}

	return liveEvents.some((event) => {
		const handoff =
			"sessionPlanHandedOff" in event ? event.sessionPlanHandedOff : null;
		return Boolean(
			handoff &&
				matches({
					sessionId: handoff.session_id,
					planMessageId: handoff.plan_message_id,
					planVersion: handoff.plan_version,
					planHash: handoff.plan_hash,
				}),
		);
	});
}
