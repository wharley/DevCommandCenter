import type { CoreEvent, TurnReviewFile, TurnReviewSummary } from "@dcc/contracts";

export type TurnReviewIdentity = {
	sessionId: string;
	workspaceId: string;
};

export type TurnReviewInvalidationCursor = {
	identity: string | null;
	terminalEvent: string | null;
};

export function reconcileTurnReviewSelection(
	selectedPath: string | null,
	files: TurnReviewFile[],
): string | null {
	if (selectedPath && files.some((file) => file.path === selectedPath)) {
		return selectedPath;
	}
	return (
		files.find((file) => !file.previewUnavailable)?.path ??
		files[0]?.path ??
		null
	);
}

export function latestTurnReviewTerminalEvent(
	events: CoreEvent[],
	sessionId: string,
): string | null {
	for (let index = events.length - 1; index >= 0; index -= 1) {
		const event = events[index];
		if (
			"sessionTurnCompleted" in event &&
			event.sessionTurnCompleted?.session_id === sessionId
		) {
			return `completed:${event.sessionTurnCompleted.turn_id}`;
		}
		if (
			"sessionTurnAborted" in event &&
			event.sessionTurnAborted?.session_id === sessionId
		) {
			return `aborted:${event.sessionTurnAborted.turn_id}`;
		}
	}
	return null;
}

export function shouldInvalidateTurnReview(
	previous: TurnReviewInvalidationCursor,
	next: TurnReviewInvalidationCursor,
): boolean {
	return Boolean(
		next.identity &&
			previous.identity === next.identity &&
			next.terminalEvent &&
			previous.terminalEvent !== next.terminalEvent,
	);
}

export function isTurnReviewIdentityActive(
	review: TurnReviewIdentity,
	active: TurnReviewIdentity | null,
): boolean {
	return Boolean(
		active &&
			review.sessionId === active.sessionId &&
			review.workspaceId === active.workspaceId,
	);
}

export type TurnReviewPreviewState =
	| "idle"
	| "loading"
	| "error"
	| "diff"
	| "unavailable";

export type TurnReviewOutcomePresentation = {
	outcome: "completed" | "aborted";
	reason: "interrupted" | "timeout" | "provider" | "other" | null;
};

export type GuardedUndoCapturePresentation = {
	state:
		| "eligible"
		| "ineligible"
		| "failed"
		| "collecting"
		| "expired"
		| "consumed"
		| "unavailable";
	reason:
		| "capture_v2_missing"
		| "protected"
		| "collecting"
		| "expired"
		| "consumed"
		| "interrupted"
		| "unsupported"
		| "limit"
		| "storage"
		| "ineligible";
};

export function resolveGuardedUndoCapture(
	capture: TurnReviewSummary["guardedUndo"],
): GuardedUndoCapturePresentation {
	if (!capture) return { state: "unavailable", reason: "capture_v2_missing" };
	const state = [
		"eligible",
		"ineligible",
		"failed",
		"collecting",
		"expired",
		"consumed",
	].includes(capture.state)
		? (capture.state as GuardedUndoCapturePresentation["state"])
		: "failed";
	if (state === "failed" && capture.state !== "failed") {
		return { state, reason: "ineligible" };
	}
	const reason = capture.reasonCode;
	if (!reason) {
		return {
			state,
			reason:
				state === "eligible"
					? "protected"
					: state === "collecting"
						? "collecting"
						: state === "expired"
							? "expired"
							: state === "consumed"
								? "consumed"
								: "ineligible",
		};
	}
	if (
		[
			"capture_interrupted",
			"operation_interrupted",
			"capture_timeout",
			"capture_race",
			"concurrent_workspace_mutation",
			"mutation_in_progress",
		].includes(reason ?? "")
	) {
		return { state, reason: "interrupted" };
	}
	if (
		[
			"adapter_unsupported",
			"filesystem_unsupported",
			"extended_metadata_unsupported",
			"symlink_or_reparse_point",
			"hardlink_unsupported",
			"non_regular_file",
			"submodule",
			"sparse_or_skip_worktree",
		].includes(reason ?? "")
	) {
		return { state, reason: "unsupported" };
	}
	if (
		[
			"file_too_large",
			"set_too_large",
			"baseline_too_large",
			"too_many_files",
			"too_many_baseline_files",
			"insufficient_disk_space",
			"index_too_large",
		].includes(reason ?? "")
	) {
		return { state, reason: "limit" };
	}
	if (
		[
			"artifact_missing",
			"artifact_corrupt",
			"artifact_store_unsafe",
			"app_instance_conflict",
			"permission_denied",
			"io_error",
		].includes(reason ?? "")
	) {
		return { state, reason: "storage" };
	}
	return { state, reason: "ineligible" };
}

export function resolveTurnReviewOutcome(
	turnOutcome: unknown,
	outcomeReason: unknown,
): TurnReviewOutcomePresentation | null {
	if (turnOutcome !== "completed" && turnOutcome !== "aborted") return null;
	if (turnOutcome === "completed") {
		return { outcome: "completed", reason: null };
	}
	if (typeof outcomeReason !== "string" || !outcomeReason.trim()) {
		return { outcome: "aborted", reason: "other" };
	}
	const normalized = outcomeReason.toLocaleLowerCase("en-US");
	if (/timeout|timed out|tempo limite|expir/.test(normalized)) {
		return { outcome: "aborted", reason: "timeout" };
	}
	if (/interrupt|abort|cancel|cancelad|interromp/.test(normalized)) {
		return { outcome: "aborted", reason: "interrupted" };
	}
	if (/provider|model|process|exit|failed|error|falh/.test(normalized)) {
		return { outcome: "aborted", reason: "provider" };
	}
	return { outcome: "aborted", reason: "other" };
}

export function resolveTurnReviewPreviewState(input: {
	selectedPath: string | null;
	isFetching: boolean;
	isError: boolean;
	diff: string | null | undefined;
}): TurnReviewPreviewState {
	if (!input.selectedPath) return "idle";
	if (input.isFetching) return "loading";
	if (input.isError) return "error";
	if (input.diff) return "diff";
	return "unavailable";
}
