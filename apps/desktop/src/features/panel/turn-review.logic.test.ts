import { describe, expect, it } from "vitest";
import {
	canPrepareGuardedUndo,
	isGuardedUndoPreviewExpired,
	resolveTurnReviewOutcome,
	resolveGuardedUndoCapture,
	resolveGuardedUndoFailureReason,
} from "./turn-review.logic";
import { visibleTurnReviewSummary } from "./turn-review-query";

describe("turn review presentation", () => {
	it("hides a cached previous summary while the latest turn refetches", () => {
		const cached = { snapshotId: "previous-turn" } as never;
		expect(visibleTurnReviewSummary(cached, true)).toBeNull();
		expect(visibleTurnReviewSummary(cached, false)).toBe(cached);
	});
	it("maps outcome reasons to safe categories without returning backend text", () => {
		expect(resolveTurnReviewOutcome("completed", "ignored detail")).toEqual({
			outcome: "completed",
			reason: null,
		});
		expect(resolveTurnReviewOutcome("aborted", "Interrupted by user")).toEqual({
			outcome: "aborted",
			reason: "interrupted",
		});
		expect(resolveTurnReviewOutcome("aborted", "provider process exited")).toEqual({
			outcome: "aborted",
			reason: "provider",
		});
		expect(resolveTurnReviewOutcome("aborted", "secret/path/token")).toEqual({
			outcome: "aborted",
			reason: "other",
		});
		expect(resolveTurnReviewOutcome("unexpected", "anything")).toBeNull();
	});

	it("shows an explicit unavailable status when capture v2 is absent", () => {
		expect(resolveGuardedUndoCapture(null)).toEqual({
			state: "unavailable",
			reason: "capture_v2_missing",
		});
	});

	it("maps guarded undo state and reason without exposing artifact metadata", () => {
		expect(
			resolveGuardedUndoCapture({
				state: "ineligible",
				reasonCode: "hardlink_unsupported",
				fileCount: 0,
				artifactBytes: 0,
				completedAt: "t1",
				expiresAt: null,
			}),
		).toEqual({ state: "ineligible", reason: "unsupported" });
	});

	it.each([
		["eligible", "protected"],
		["collecting", "collecting"],
		["expired", "expired"],
		["consumed", "consumed"],
	] as const)("maps a null reason for %s to its own safe presentation", (state, reason) => {
		expect(
			resolveGuardedUndoCapture({
				state,
				reasonCode: null,
				fileCount: 2,
				artifactBytes: 12,
				completedAt: "t1",
				expiresAt: state === "eligible" ? "t2" : null,
			}),
		).toEqual({ state, reason });
	});

	it("fails closed when the persisted guarded undo state is unknown", () => {
		expect(
			resolveGuardedUndoCapture({
				state: "future_state",
				reasonCode: null,
				fileCount: 0,
				artifactBytes: 0,
				completedAt: null,
				expiresAt: null,
			}),
		).toEqual({ state: "failed", reason: "ineligible" });
	});

	it("enables prepare only for an eligible capture without an active operation", () => {
		const eligible = {
			state: "eligible",
			reasonCode: null,
			fileCount: 2,
			artifactBytes: 12,
			completedAt: "t1",
			expiresAt: "t2",
		};
		expect(canPrepareGuardedUndo(eligible, null)).toBe(true);
		expect(
			canPrepareGuardedUndo(eligible, {
				status: "recovery_required",
				operationId: "operation-1",
				reasonCode: "manual_recovery_required",
			}),
		).toBe(false);
		expect(canPrepareGuardedUndo({ ...eligible, state: "consumed" }, null)).toBe(
			false,
		);
	});

	it.each([
		["preview_expired", "expired"],
		["head_changed", "changed"],
		["artifact_corrupt", "corrupt"],
		["manual_recovery_required", "recovery"],
		["mutation_in_progress", "busy"],
		["filesystem_unsupported", "unsupported"],
		["future_reason", "unavailable"],
	] as const)("maps guarded undo reason %s to %s", (reason, expected) => {
		expect(resolveGuardedUndoFailureReason(reason)).toBe(expected);
	});

	it("fails closed for invalid or expired preview timestamps", () => {
		const now = Date.parse("2026-08-28T12:00:00Z");
		expect(isGuardedUndoPreviewExpired("2026-08-28T12:00:01Z", now)).toBe(false);
		expect(isGuardedUndoPreviewExpired("2026-08-28T12:00:00Z", now)).toBe(true);
		expect(isGuardedUndoPreviewExpired("invalid", now)).toBe(true);
	});
});
