/**
 * When each room was last opened on this phone (`@dcc/mobile-core` rooms),
 * plus the baseline — the first time this phone tracked rooms, so answers
 * from before are not news. Per device, kept with the other preferences.
 */
import { roomKey, type RoomsSeen } from "@dcc/mobile-core";
import { getItem, setItem } from "./secureStorage";

const KEY = "dcc.roomsSeen";
/** Keychain items stay small. */
const MAX_ENTRIES = 40;

export type StoredRoomsSeen = { baseline: string | null; seen: RoomsSeen };

let state: StoredRoomsSeen | null = null;
let loading: Promise<StoredRoomsSeen> | null = null;
const listeners = new Set<(value: StoredRoomsSeen) => void>();

function save(next: StoredRoomsSeen) {
	state = next;
	for (const listener of [...listeners]) listener(next);
	void setItem(KEY, JSON.stringify(next)).catch(() => {});
}

export function loadRoomsSeen(): Promise<StoredRoomsSeen> {
	if (state) return Promise.resolve(state);
	loading ??= getItem(KEY)
		.catch(() => null)
		.then((raw) => {
			let parsed: StoredRoomsSeen | null = null;
			try {
				parsed = raw ? (JSON.parse(raw) as StoredRoomsSeen) : null;
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

export function onRoomsSeen(listener: (value: StoredRoomsSeen) => void): () => void {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

/** The room is open and read up to `at` (its latest line), or now. */
export function markRoomSeen(id: string | null, at: string | null = null): void {
	void loadRoomsSeen().then((current) => {
		const key = roomKey(id);
		const now = new Date().toISOString();
		const when = at && at > now ? at : now;
		if ((current.seen[key] ?? "") >= when) return;
		const entries = Object.entries({ ...current.seen, [key]: when })
			.sort((left, right) => right[1].localeCompare(left[1]))
			.slice(0, MAX_ENTRIES);
		save({ baseline: current.baseline, seen: Object.fromEntries(entries) });
	});
}
