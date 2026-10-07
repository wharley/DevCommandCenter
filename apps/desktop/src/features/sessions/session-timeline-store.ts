import type { CoreEvent, SessionEventRecord } from "@dcc/contracts";
import { create } from "zustand";

import type { SessionLiveReconcileState } from "./session-live-reconciler";

/** Conversations kept in memory so switching back to them is instant. */
export const SESSION_TIMELINE_CACHE_LIMIT = 6;

export type SessionTimelineEntry = {
	sessionId: string;
	history: SessionEventRecord[];
	liveEvents: CoreEvent[];
	/** There is something to show (a snapshot, possibly being refreshed). */
	ready: boolean;
	/** False once the live transport failed and the legacy feed took over. */
	active: boolean;
	/** A newer snapshot is loading; the entry still shows the previous one. */
	refreshing: boolean;
	touchedAt: number;
};

type SessionTimelineStore = {
	entries: Record<string, SessionTimelineEntry>;
	/** Takes the reconciler's state; a reset never blanks a timeline already shown. */
	publish: (state: SessionLiveReconcileState) => void;
	fallBackToLegacy: (sessionId: string) => void;
	clear: () => void;
};

let clock = 0;

function evict(
	entries: Record<string, SessionTimelineEntry>,
	keep: string,
): Record<string, SessionTimelineEntry> {
	const ids = Object.keys(entries);
	if (ids.length <= SESSION_TIMELINE_CACHE_LIMIT) return entries;
	const victims = ids
		.filter((id) => id !== keep)
		.sort((left, right) => entries[left]!.touchedAt - entries[right]!.touchedAt)
		.slice(0, ids.length - SESSION_TIMELINE_CACHE_LIMIT);
	const next = { ...entries };
	for (const id of victims) delete next[id];
	return next;
}

export const useSessionTimelineStore = create<SessionTimelineStore>()((set) => ({
	entries: {},
	publish: (state) =>
		set(({ entries }) => {
			const sessionId = state.sessionId;
			if (!sessionId) return {};
			const previous = entries[sessionId];
			let entry: SessionTimelineEntry;
			if (state.ready) {
				entry = {
					sessionId,
					history: state.history,
					liveEvents: state.liveEvents,
					ready: true,
					active: true,
					refreshing: false,
					touchedAt: ++clock,
				};
			} else if (previous?.ready && previous.active) {
				// Rehydrating (a new runtime generation, a gap in the live feed):
				// keep the conversation on screen until the new snapshot lands.
				if (previous.refreshing) return {};
				entry = { ...previous, refreshing: true, touchedAt: ++clock };
			} else {
				entry = {
					sessionId,
					history: [],
					liveEvents: [],
					ready: false,
					active: true,
					refreshing: false,
					touchedAt: ++clock,
				};
			}
			return { entries: evict({ ...entries, [sessionId]: entry }, sessionId) };
		}),
	fallBackToLegacy: (sessionId) =>
		set(({ entries }) => ({
			entries: {
				...entries,
				[sessionId]: {
					sessionId,
					history: [],
					liveEvents: [],
					ready: false,
					active: false,
					refreshing: false,
					touchedAt: ++clock,
				},
			},
		})),
	clear: () => set({ entries: {} }),
}));
