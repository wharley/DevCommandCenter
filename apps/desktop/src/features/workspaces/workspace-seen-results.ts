import { useSyncExternalStore } from "react";

/**
 * Which finished task results the person has already looked at, per workspace.
 * Kept in this browser profile: it is a reading marker, not task state.
 *
 * A baseline is stamped the first time the marker is read, so results that
 * finished before this feature existed do not all show up as unread at once.
 */
const STORAGE_KEY = "dcc.workspaces.seenResults";
const BASELINE_KEY = "dcc.workspaces.seenResultsBaseline";
const MAX_ENTRIES = 300;

export type WorkspaceSeenResults = {
	baseline: string | null;
	seen: Record<string, string>;
};

const listeners = new Set<() => void>();
let cache: WorkspaceSeenResults | null = null;

function timestampMs(value: string | null | undefined): number {
	if (!value) return Number.NaN;
	return Date.parse(value);
}

function read(): WorkspaceSeenResults {
	if (cache) {
		return cache;
	}
	let seen: Record<string, string> = {};
	let baseline: string | null = null;
	try {
		const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
		seen = parsed && typeof parsed === "object" ? (parsed as Record<string, string>) : {};
		baseline = window.localStorage.getItem(BASELINE_KEY);
		if (!baseline) {
			baseline = new Date().toISOString();
			window.localStorage.setItem(BASELINE_KEY, baseline);
		}
	} catch {
		// Storage can be unavailable; without a baseline nothing counts as unread.
	}
	cache = { baseline, seen };
	return cache;
}

/**
 * A finished result is unread when it finished after the baseline and after
 * the last result the person saw for that workspace. Without a baseline
 * (storage unavailable) nothing is unread — better quiet than noisy.
 */
export function isWorkspaceResultUnread(
	completedAt: string | null | undefined,
	seenAt: string | undefined,
	baseline: string | null,
): boolean {
	const completedMs = timestampMs(completedAt);
	const baselineMs = timestampMs(baseline);
	if (Number.isNaN(completedMs) || Number.isNaN(baselineMs) || completedMs <= baselineMs) {
		return false;
	}
	const seenMs = timestampMs(seenAt);
	return Number.isNaN(seenMs) || seenMs < completedMs;
}

/** Keeps the newest entries so the marker cannot grow without bound. */
export function withSeenWorkspaceResult(
	seen: Record<string, string>,
	workspaceId: string,
	completedAt: string,
	maxEntries = MAX_ENTRIES,
): Record<string, string> {
	const entries = Object.entries({ ...seen, [workspaceId]: completedAt })
		.sort(([, left], [, right]) => timestampMs(right) - timestampMs(left))
		.slice(0, maxEntries);
	return Object.fromEntries(entries);
}

export function markWorkspaceResultSeen(workspaceId: string, completedAt: string): void {
	const current = read();
	if (!isWorkspaceResultUnread(completedAt, current.seen[workspaceId], current.baseline)) {
		return;
	}
	cache = {
		baseline: current.baseline,
		seen: withSeenWorkspaceResult(current.seen, workspaceId, completedAt),
	};
	try {
		window.localStorage.setItem(STORAGE_KEY, JSON.stringify(cache.seen));
	} catch {
		// Storage can be unavailable or full; the marker then lasts for this run.
	}
	for (const listener of listeners) {
		listener();
	}
}

export function useSeenWorkspaceResults(): WorkspaceSeenResults {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		read,
		read,
	);
}
