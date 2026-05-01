import type { RuntimeSessionSnapshot } from "./workbench-types";

/** Resume applies when the persisted session is not already active (core `resume_session`). */
export function canResumeSession(snapshot: RuntimeSessionSnapshot | null): boolean {
	if (!snapshot) {
		return false;
	}
	return snapshot.state !== "active";
}

/**
 * Abort applies only for an active session with a running turn (or request still in flight).
 * Mirrors core `abort_run`: active projection + active_turn_id.
 */
export function canAbortRun(
	snapshot: RuntimeSessionSnapshot | null,
	pendingPrompt: string | null,
): boolean {
	if (!snapshot || snapshot.state !== "active") {
		return false;
	}
	if (pendingPrompt != null && pendingPrompt.length > 0) {
		return true;
	}
	return Boolean(snapshot.activeTurnId && snapshot.activeTurnId.length > 0);
}
