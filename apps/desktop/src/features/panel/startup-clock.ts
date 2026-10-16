import { useSyncExternalStore } from "react";

/**
 * When the person pressed send, so the clock on the startup row and the
 * "Working · 59s" line that replaces it count from the same moment. The
 * backend only knows when the turn started, which is after model routing,
 * session creation and MCP preflight.
 *
 * Client memory only: after a restart, turns fall back to their TurnStarted.
 */
type PendingPromptClock = { at: number; sessionId: string | null };

/** A send never takes this long to become a turn; older clocks are stale. */
const MAX_STARTUP_MS = 5 * 60_000;
/** Backend and UI share the machine clock, but timestamps round differently. */
const CLOCK_SKEW_MS = 2_000;

let pending: PendingPromptClock | null = null;
const originsByTurnId = new Map<string, number>();
const listeners = new Set<() => void>();

function emit() {
	for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

/** Starts the clock for the prompt being sent from `sessionId` (null: a new conversation). */
export function startPromptClock(sessionId: string | null, at = Date.now()) {
	pending = { at, sessionId };
	emit();
}

/**
 * Hands the pending clock to the turn the prompt became. A turn that started
 * before the send, or long after it, belongs to someone else.
 */
export function bindPromptClock(input: {
	turnId: string;
	sessionId: string | null;
	turnStartedAt: string | undefined;
}) {
	if (!pending || originsByTurnId.has(input.turnId)) return;
	if (pending.sessionId && pending.sessionId !== input.sessionId) return;
	const startedAt = input.turnStartedAt ? Date.parse(input.turnStartedAt) : Number.NaN;
	if (!Number.isFinite(startedAt)) return;
	if (startedAt < pending.at - CLOCK_SKEW_MS || startedAt - pending.at > MAX_STARTUP_MS) return;
	originsByTurnId.set(input.turnId, pending.at);
	pending = null;
	emit();
}

export function usePendingPromptClock(sessionId: string | null): number | null {
	const clock = useSyncExternalStore(subscribe, () => pending);
	if (!clock || (clock.sessionId && clock.sessionId !== sessionId)) return null;
	return Date.now() - clock.at <= MAX_STARTUP_MS ? clock.at : null;
}

/** When the person sent the prompt that started `turnId`, if this window saw it. */
export function turnClockOrigin(turnId: string | null | undefined): number | null {
	return turnId ? (originsByTurnId.get(turnId) ?? null) : null;
}

export function useTurnClockOrigin(turnId: string | null | undefined): number | null {
	return useSyncExternalStore(subscribe, () => turnClockOrigin(turnId));
}

/** The turn's start as the person saw it: the send, when that came first. */
export function turnStartedAtWithOrigin(
	turnStartedAt: string | undefined,
	origin: number | null,
): string | undefined {
	if (origin == null) return turnStartedAt;
	const started = turnStartedAt ? Date.parse(turnStartedAt) : Number.NaN;
	if (Number.isFinite(started) && started <= origin) return turnStartedAt;
	return new Date(origin).toISOString();
}

export function resetStartupClockForTests() {
	pending = null;
	originsByTurnId.clear();
	emit();
}
