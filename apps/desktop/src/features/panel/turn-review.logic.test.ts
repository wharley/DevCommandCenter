import type { CoreEvent, TurnReviewFile } from "@dcc/contracts";
import { describe, expect, it } from "vitest";
import {
	isTurnReviewIdentityActive,
	latestTurnReviewTerminalEvent,
	reconcileTurnReviewSelection,
	resolveTurnReviewOutcome,
	resolveGuardedUndoCapture,
	resolveTurnReviewPreviewState,
	shouldInvalidateTurnReview,
} from "./turn-review.logic";
import { visibleTurnReviewSummary } from "./turn-review-query";

const file = (
	path: string,
	previewUnavailable = false,
): TurnReviewFile => ({
	path,
	status: "M",
	insertions: 1,
	deletions: 0,
	previewUnavailable,
});

const event = (value: object) => value as CoreEvent;

describe("turn review presentation", () => {
	it("hides a cached previous summary while the latest turn refetches", () => {
		const cached = { snapshotId: "previous-turn" } as never;
		expect(visibleTurnReviewSummary(cached, true)).toBeNull();
		expect(visibleTurnReviewSummary(cached, false)).toBe(cached);
	});
	it("keeps a valid file selected and falls back to the first previewable file", () => {
		const files = [file("binary.png", true), file("src/app.ts")];
		expect(reconcileTurnReviewSelection("src/app.ts", files)).toBe("src/app.ts");
		expect(reconcileTurnReviewSelection("removed.ts", files)).toBe("src/app.ts");
	});

	it("selects a file when a collecting snapshot becomes ready", () => {
		expect(reconcileTurnReviewSelection(null, [])).toBeNull();
		expect(reconcileTurnReviewSelection(null, [file("src/ready.ts")])).toBe(
			"src/ready.ts",
		);
	});

	it("uses an unavailable file only when no text preview exists", () => {
		expect(reconcileTurnReviewSelection(null, [file("asset.bin", true)])).toBe(
			"asset.bin",
		);
	});

	it("does not show a loading preview without a selected path", () => {
		expect(
			resolveTurnReviewPreviewState({
				selectedPath: null,
				isFetching: true,
				isError: false,
				diff: undefined,
			}),
		).toBe("idle");
	});

	it.each([
		[true, false, undefined, "loading"],
		[false, true, undefined, "error"],
		[false, false, "@@ diff", "diff"],
		[false, false, null, "unavailable"],
	] as const)(
		"resolves file preview states",
		(isFetching, isError, diff, expected) => {
			expect(
				resolveTurnReviewPreviewState({
					selectedPath: "src/app.ts",
					isFetching,
					isError,
					diff,
				}),
			).toBe(expected);
		},
	);

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
});

describe("turn review identity and terminal invalidation", () => {
	it("requires both the active session and active bundle member", () => {
		const review = { sessionId: "session-a", workspaceId: "member-b" };
		expect(isTurnReviewIdentityActive(review, review)).toBe(true);
		expect(
			isTurnReviewIdentityActive(review, {
				sessionId: "session-b",
				workspaceId: "member-b",
			}),
		).toBe(false);
		expect(
			isTurnReviewIdentityActive(review, {
				sessionId: "session-a",
				workspaceId: "member-a",
			}),
		).toBe(false);
	});

	it("ignores unrelated events and returns only the latest matching terminal turn", () => {
		const events = [
			event({ sessionTurnCompleted: { session_id: "session-a", turn_id: "1" } }),
			event({ sessionTurnDelta: { session_id: "session-a", turn_id: "2", content: "x" } }),
			event({ sessionTurnCompleted: { session_id: "session-b", turn_id: "9" } }),
			event({ sessionTurnAborted: { session_id: "session-a", turn_id: "2", reason: "stop" } }),
		];
		expect(latestTurnReviewTerminalEvent(events, "session-a")).toBe(
			"aborted:2",
		);
		expect(latestTurnReviewTerminalEvent(events, "missing")).toBeNull();
	});

	it("invalidates only when a terminal event advances for the same open review", () => {
		const open = { identity: "session-a:workspace-a", terminalEvent: null };
		expect(
			shouldInvalidateTurnReview(open, {
				...open,
				terminalEvent: "completed:turn-a",
			}),
		).toBe(true);
		expect(
			shouldInvalidateTurnReview(
				{ ...open, terminalEvent: "completed:turn-a" },
				{ ...open, terminalEvent: "completed:turn-a" },
			),
		).toBe(false);
		expect(
			shouldInvalidateTurnReview(open, {
				identity: "session-b:workspace-a",
				terminalEvent: "completed:turn-b",
			}),
		).toBe(false);
	});
});
