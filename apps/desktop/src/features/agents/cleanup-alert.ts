import { useSyncExternalStore } from "react";

/**
 * When the reporter speaks up about the space completed tasks take. Only the
 * limit is the person's to choose; what counts as safe to delete is DCC's.
 * Kept in this browser profile, like the recap's reading marker.
 */
const STORAGE_KEY = "dcc.agents.cleanupAlert";
const GIB = 1024 ** 3;
export const DEFAULT_CLEANUP_THRESHOLD_GB = 5;
export const MIN_CLEANUP_THRESHOLD_GB = 1;
export const MAX_CLEANUP_THRESHOLD_GB = 1000;
/** After the bubble is closed, it comes back only once completed tasks grow this much more. */
export const CLEANUP_REALERT_GROWTH_BYTES = GIB;

export type CleanupAlertState = {
	enabled: boolean;
	thresholdGb: number;
	/** Total when the person closed the bubble; cleared once the total drops below the limit. */
	dismissedAtBytes: number | null;
};

const DEFAULT_STATE: CleanupAlertState = {
	enabled: true,
	thresholdGb: DEFAULT_CLEANUP_THRESHOLD_GB,
	dismissedAtBytes: null,
};

export function clampCleanupThresholdGb(value: number): number {
	if (!Number.isFinite(value)) return DEFAULT_CLEANUP_THRESHOLD_GB;
	return Math.min(MAX_CLEANUP_THRESHOLD_GB, Math.max(MIN_CLEANUP_THRESHOLD_GB, Math.round(value)));
}

export function isCleanupAlertDue(state: CleanupAlertState, totalBytes: number): boolean {
	if (!state.enabled || totalBytes < state.thresholdGb * GIB) {
		return false;
	}
	return (
		state.dismissedAtBytes === null ||
		totalBytes >= state.dismissedAtBytes + CLEANUP_REALERT_GROWTH_BYTES
	);
}

/** A closed bubble stays closed until the total falls under the limit and crosses it again. */
export function settleCleanupAlert(
	state: CleanupAlertState,
	totalBytes: number,
): CleanupAlertState {
	if (state.dismissedAtBytes !== null && totalBytes < state.thresholdGb * GIB) {
		return { ...state, dismissedAtBytes: null };
	}
	return state;
}

const listeners = new Set<() => void>();
let cache: CleanupAlertState | null = null;

function read(): CleanupAlertState {
	if (cache) {
		return cache;
	}
	try {
		const parsed = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<
			CleanupAlertState
		> | null;
		cache = {
			enabled: typeof parsed?.enabled === "boolean" ? parsed.enabled : DEFAULT_STATE.enabled,
			thresholdGb:
				typeof parsed?.thresholdGb === "number"
					? clampCleanupThresholdGb(parsed.thresholdGb)
					: DEFAULT_STATE.thresholdGb,
			dismissedAtBytes:
				typeof parsed?.dismissedAtBytes === "number" ? parsed.dismissedAtBytes : null,
		};
	} catch {
		cache = DEFAULT_STATE;
	}
	return cache;
}

function write(next: CleanupAlertState): void {
	cache = next;
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
	} catch {
		// Storage can be unavailable or full; the setting then lasts for this run.
	}
	for (const listener of listeners) {
		listener();
	}
}

export function readCleanupAlert(): CleanupAlertState {
	return read();
}

export function setCleanupAlertSettings(settings: { enabled: boolean; thresholdGb: number }): void {
	const state = read();
	const thresholdGb = clampCleanupThresholdGb(settings.thresholdGb);
	if (state.enabled !== settings.enabled || state.thresholdGb !== thresholdGb) {
		write({ ...state, enabled: settings.enabled, thresholdGb });
	}
}

export function dismissCleanupAlert(totalBytes: number): void {
	write({ ...read(), dismissedAtBytes: totalBytes });
}

export function syncCleanupAlert(totalBytes: number): void {
	const state = read();
	const next = settleCleanupAlert(state, totalBytes);
	if (next !== state) {
		write(next);
	}
}

export function useCleanupAlert(): CleanupAlertState {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		read,
		read,
	);
}
