/**
 * Finished results opened on this phone (`@dcc/mobile-core` attention):
 * workspace → when its thread was last opened, plus the baseline — the first
 * time this phone tracked results, so older ones are not news. Per device,
 * like the desktop's localStorage marker; kept with the other preferences.
 */
import { withSeenResult, type SeenResults } from "@dcc/mobile-core";
import { getItem, setItem } from "./secureStorage";

const KEY = "dcc.seenResults";
/** Keychain items stay small. */
const MAX_ENTRIES = 60;

type Stored = { baseline: string | null; seen: SeenResults };

let state: Stored | null = null;
let loading: Promise<Stored> | null = null;
const listeners = new Set<(value: Stored) => void>();

function save(next: Stored) {
	state = next;
	for (const listener of [...listeners]) listener(next);
	void setItem(KEY, JSON.stringify(next)).catch(() => {});
}

export function loadSeenResults(): Promise<Stored> {
	if (state) return Promise.resolve(state);
	loading ??= getItem(KEY)
		.catch(() => null)
		.then((raw) => {
			let parsed: Stored | null = null;
			try {
				parsed = raw ? (JSON.parse(raw) as Stored) : null;
			} catch {
				parsed = null;
			}
			if (parsed && typeof parsed.baseline === "string") {
				state = { baseline: parsed.baseline, seen: parsed.seen ?? {} };
			} else {
				save({ baseline: new Date().toISOString(), seen: {} });
			}
			return state!;
		});
	return loading;
}

export function onSeenResults(listener: (value: Stored) => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** Opening a task's thread reads its latest result. */
export function markWorkspaceSeen(workspaceId: string, at = new Date().toISOString()): void {
	void loadSeenResults().then((current) => {
		const seen = Object.fromEntries(Object.entries(withSeenResult(current.seen, workspaceId, at)).slice(0, MAX_ENTRIES));
		save({ baseline: current.baseline, seen });
	});
}
