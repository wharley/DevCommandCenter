import type { SessionUsageLimit } from "@dcc/contracts";

/**
 * A task stopped by its plan's usage limit: waiting for the reset
 * ("Aguardando limite") or for the person ("Pausado por limite"). Mirrors
 * `usage_limit_phase` in `crates/dcc-core/src/domain/usage_limit.rs`; the
 * shared cases live in `packages/contracts/fixtures/attention-vectors.json`.
 */
export type UsageLimitPhase = "waiting" | "paused";

export type UsageLimitPauseReason =
	| "unknown_reset"
	| "reset_passed"
	| "attempts_exhausted";

export type UsageLimitStatus = {
	phase: UsageLimitPhase;
	pauseReason: UsageLimitPauseReason | null;
	/** When the person gets it back: the reset for a reopened window, else the stop. */
	returnedAt: string;
};

/** Automatic resumes per task in 12 hours (`MAX_AUTO_RESUMES`). */
export const MAX_AUTO_RESUMES = 3;

export function usageLimitStatus(
	limit: SessionUsageLimit,
	now: number = Date.now(),
): UsageLimitStatus {
	const resetsAtMs = limit.resetsAt ? Date.parse(limit.resetsAt) : Number.NaN;
	const paused = (pauseReason: UsageLimitPauseReason): UsageLimitStatus => ({
		phase: "paused",
		pauseReason,
		returnedAt:
			pauseReason === "reset_passed" && limit.resetsAt ? limit.resetsAt : limit.hitAt,
	});
	if (Number.isNaN(resetsAtMs)) return paused("unknown_reset");
	if (limit.autoResume && limit.autoResumesUsed >= MAX_AUTO_RESUMES) {
		return paused("attempts_exhausted");
	}
	if (now < resetsAtMs || limit.autoResume) {
		return { phase: "waiting", pauseReason: null, returnedAt: limit.hitAt };
	}
	return paused("reset_passed");
}

/** "03:40" in the person's clock; with the date when it is not today. */
export function formatLimitReset(
	resetsAt: string | null | undefined,
	now: number = Date.now(),
	locale = "pt-BR",
): string | null {
	if (!resetsAt) return null;
	const at = new Date(resetsAt);
	if (Number.isNaN(at.getTime())) return null;
	const time = at.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
	const today = new Date(now);
	const sameDay =
		at.getFullYear() === today.getFullYear() &&
		at.getMonth() === today.getMonth() &&
		at.getDate() === today.getDate();
	if (sameDay) return time;
	const day = at.toLocaleDateString(locale, { day: "2-digit", month: "2-digit" });
	return `${day} ${time}`;
}
