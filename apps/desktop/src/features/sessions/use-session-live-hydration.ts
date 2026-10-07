import { useEffect, useMemo, useRef } from "react";

import {
	listenSessionLiveEvents,
	loadSessionLiveSnapshot,
} from "@/lib/session-api";

import { SessionLiveReconciler } from "./session-live-reconciler";
import { useSessionTimelineStore, type SessionTimelineEntry } from "./session-timeline-store";

const MAX_CONSECUTIVE_REHYDRATES = 2;

function inactiveState(sessionId: string | null): SessionTimelineEntry {
	return {
		sessionId: sessionId ?? "",
		history: [],
		liveEvents: [],
		ready: false,
		active: Boolean(sessionId),
		refreshing: false,
		touchedAt: 0,
	};
}

/**
 * Subscribes before every durable snapshot. The request generation and
 * unsubscribe cleanup make session/workspace changes fail closed without
 * polling or persisting runtime identity.
 *
 * Reconciled timelines live in a session-keyed store: revisiting a recent
 * conversation shows it at once while a fresh snapshot loads behind it, and
 * a rehydrate keeps the current conversation on screen instead of blanking it.
 */
export function useSessionLiveHydration(sessionId: string | null) {
	const requestRef = useRef(0);
	const entry = useSessionTimelineStore((store) =>
		sessionId ? store.entries[sessionId] : undefined,
	);
	const fallback = useMemo(() => inactiveState(sessionId), [sessionId]);

	useEffect(() => {
		const request = ++requestRef.current;
		if (!sessionId) return;
		const { publish: publishState, fallBackToLegacy, entries } =
			useSessionTimelineStore.getState();
		// A conversation past the snapshot limits fails the same way on every
		// visit; it stays on the legacy feed instead of retrying each switch.
		if (entries[sessionId]?.active === false) return;

		let disposed = false;
		let unlisten: (() => void) | null = null;
		let fetching = false;
		let queuedRehydrate = false;
		let rehydrateAttempts = 0;
		let frame: number | null = null;
		let legacyOnly = false;
		const reconciler = new SessionLiveReconciler(sessionId);
		publishState(reconciler.current());

		const publish = () => {
			if (disposed || request !== requestRef.current) return;
			if (frame !== null) return;
			frame = requestAnimationFrame(() => {
				frame = null;
				if (disposed || request !== requestRef.current) return;
				publishState(reconciler.current());
			});
		};
		const fallbackToLegacy = () => {
			if (disposed || request !== requestRef.current) return;
			legacyOnly = true;
			if (frame !== null) {
				cancelAnimationFrame(frame);
				frame = null;
			}
			fallBackToLegacy(sessionId);
			unlisten?.();
		};

		const hydrate = async () => {
			if (legacyOnly || disposed || request !== requestRef.current) {
				return;
			}
			if (fetching) {
				queuedRehydrate = true;
				return;
			}
			fetching = true;
			queuedRehydrate = false;
			reconciler.beginHydration();
			publish();
			try {
				const snapshot = await loadSessionLiveSnapshot(sessionId);
				if (legacyOnly || disposed || request !== requestRef.current) return;
				const result = reconciler.acceptSnapshot(snapshot);
				publish();
				if (result.rehydrate) {
					rehydrateAttempts += 1;
					if (rehydrateAttempts > MAX_CONSECUTIVE_REHYDRATES) {
						fallbackToLegacy();
						return;
					}
					queuedRehydrate = true;
				} else {
					rehydrateAttempts = 0;
				}
			} catch (error) {
				if (!disposed) {
					console.warn("[dcc] failed to hydrate live session events:", error);
				}
				fallbackToLegacy();
			} finally {
				fetching = false;
				if (queuedRehydrate && !disposed && request === requestRef.current) {
					void hydrate();
				}
			}
		};

		void listenSessionLiveEvents((envelope) => {
			if (legacyOnly || disposed || request !== requestRef.current) return;
			const result = reconciler.acceptEnvelope(envelope);
			if (result.changed) publish();
			if (result.rehydrate) {
				rehydrateAttempts += 1;
				if (rehydrateAttempts > MAX_CONSECUTIVE_REHYDRATES) {
					fallbackToLegacy();
					return;
				}
				void hydrate();
			}
		})
			.then((cleanup) => {
				if (legacyOnly || disposed || request !== requestRef.current) {
					void cleanup();
					return;
				}
				unlisten = cleanup;
				void hydrate();
			})
			.catch((error) => {
				if (!disposed) {
					console.error("[dcc] failed to subscribe to session live events:", error);
				}
				fallbackToLegacy();
			});

		return () => {
			disposed = true;
			requestRef.current += 1;
			reconciler.dispose();
			if (frame !== null) cancelAnimationFrame(frame);
			unlisten?.();
		};
	}, [sessionId]);

	return entry ?? fallback;
}
