import type { TurnReviewSummary } from "@dcc/contracts";

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

export type GuardedUndoFailurePresentation =
	| "expired"
	| "changed"
	| "corrupt"
	| "recovery"
	| "busy"
	| "unsupported"
	| "unavailable";

export function canPrepareGuardedUndo(
	capture: TurnReviewSummary["guardedUndo"],
	activeUndo: TurnReviewSummary["activeUndo"],
): boolean {
	return capture?.state === "eligible" && !activeUndo;
}

export function resolveGuardedUndoFailureReason(
	reasonCode: string | null | undefined,
): GuardedUndoFailurePresentation {
	if (
		["retention_expired", "preview_expired", "preview_consumed"].includes(
			reasonCode ?? "",
		)
	) {
		return "expired";
	}
	if (
		[
			"head_changed",
			"ref_changed",
			"index_changed",
			"repository_identity_changed",
			"target_missing",
			"target_result_mismatch",
			"metadata_changed",
			"git_attributes_changed",
			"tracked_manifest_changed",
			"preview_context_changed",
		].includes(reasonCode ?? "")
	) {
		return "changed";
	}
	if (
		[
			"artifact_missing",
			"artifact_corrupt",
			"displaced_file_missing",
			"displaced_file_corrupt",
		].includes(reasonCode ?? "")
	) {
		return "corrupt";
	}
	if (
		[
			"displaced_target_mismatch",
			"recovery_target_changed",
			"exchange_rollback_failed",
			"manual_recovery_required",
			"operation_interrupted",
		].includes(reasonCode ?? "")
	) {
		return "recovery";
	}
	if (
		[
			"mutation_in_progress",
			"concurrent_workspace_mutation",
			"app_instance_conflict",
		].includes(reasonCode ?? "")
	) {
		return "busy";
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
		].includes(reasonCode ?? "")
	) {
		return "unsupported";
	}
	return "unavailable";
}

export function isGuardedUndoPreviewExpired(
	expiresAt: string,
	now = Date.now(),
): boolean {
	const expiresAtMs = Date.parse(expiresAt);
	return !Number.isFinite(expiresAtMs) || expiresAtMs <= now;
}

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
