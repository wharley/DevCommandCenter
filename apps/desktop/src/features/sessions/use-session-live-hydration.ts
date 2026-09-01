import { useEffect, useRef, useState } from "react";

import {
	listenSessionLiveEvents,
	loadSessionLiveSnapshot,
} from "@/lib/session-api";

import {
	SessionLiveReconciler,
	type SessionLiveReconcileState,
} from "./session-live-reconciler";

type HydrationState = SessionLiveReconcileState & { active: boolean };
const MAX_CONSECUTIVE_REHYDRATES = 2;

function inactiveState(sessionId: string | null): HydrationState {
	return {
		sessionId: sessionId ?? "",
		history: [],
		liveEvents: [],
		ready: false,
		active: Boolean(sessionId),
	};
}

/**
 * Subscribes before every durable snapshot. The request generation and
 * unsubscribe cleanup make session/workspace changes fail closed without
 * polling or persisting runtime identity.
 */
export function useSessionLiveHydration(sessionId: string | null) {
	const [state, setState] = useState<HydrationState>(() => inactiveState(sessionId));
	const requestRef = useRef(0);

	useEffect(() => {
		const request = ++requestRef.current;
		if (!sessionId) {
			setState(inactiveState(null));
			return;
		}

		let disposed = false;
		let unlisten: (() => void) | null = null;
		let fetching = false;
		let queuedRehydrate = false;
		let rehydrateAttempts = 0;
		let frame: number | null = null;
		let legacyOnly = false;
		const reconciler = new SessionLiveReconciler(sessionId);
		setState({ ...reconciler.current(), active: true });

		const publish = () => {
			if (disposed || request !== requestRef.current) return;
			if (frame !== null) return;
			frame = requestAnimationFrame(() => {
				frame = null;
				if (disposed || request !== requestRef.current) return;
				setState({ ...reconciler.current(), active: true });
			});
		};
		const fallbackToLegacy = () => {
			if (disposed || request !== requestRef.current) return;
			legacyOnly = true;
			if (frame !== null) {
				cancelAnimationFrame(frame);
				frame = null;
			}
			setState({ ...inactiveState(sessionId), active: false });
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

	return state;
}
