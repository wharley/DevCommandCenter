import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

import { SessionEventFrameBatch } from "./session-event-frame-batch";
import { SessionLiveEventBuffer } from "./session-live-event-buffer";

const MAX_ACTIVITY_EVENTS = 12;

/**
 * Subscribes to the global session event stream.
 *
 * `onEvent` fires for every event, in native arrival order, in a single React
 * batch on the next animation frame. Use it to drive per-session state (e.g.
 * snapshots) so background sessions keep updating even when their tab is not selected. The returned
 * `events` is intentionally larger than the visible activity feed because the
 * conversation projection needs every live delta until persisted history
 * catches up. `activityEvents` is the small UI-only feed.
 */
export function useSessionEventFeed(onEvent?: (event: CoreEvent) => void) {
	const bufferRef = useRef(new SessionLiveEventBuffer());
	const [bufferVersion, setBufferVersion] = useState(0);
	const [activityEvents, setActivityEvents] = useState<CoreEvent[]>([]);
	const onEventRef = useRef(onEvent);
	const frameBatchRef = useRef<SessionEventFrameBatch | null>(null);

	useEffect(() => {
		onEventRef.current = onEvent;
	}, [onEvent]);

	useEffect(() => {
		let disposed = false;
		let cleanup: (() => void) | null = null;

		bufferRef.current = new SessionLiveEventBuffer();
		setBufferVersion((version) => version + 1);
		setActivityEvents([]);
		const frameBatch = new SessionEventFrameBatch((events) => {
			if (disposed || events.length === 0) return;
			for (const event of events) onEventRef.current?.(event);
			setBufferVersion((version) => version + 1);
			setActivityEvents((current) =>
				[...current, ...events].slice(-MAX_ACTIVITY_EVENTS),
			);
		});
		frameBatchRef.current = frameBatch;

		void listenSessionEvents((event) => {
			if (disposed) {
				return;
			}
			bufferRef.current.append(event);
			frameBatch.enqueue(event);
		})
			.then((unlisten) => {
				if (disposed) {
					void unlisten();
					return;
				}
				cleanup = unlisten;
			})
			.catch((error) => {
				if (!disposed) {
					console.error("[dcc] failed to subscribe to session events:", error);
				}
			});

		return () => {
			disposed = true;
			frameBatch.dispose();
			if (frameBatchRef.current === frameBatch) frameBatchRef.current = null;
			cleanup?.();
		};
	}, []);

	const events = useMemo(() => bufferRef.current.events(), [bufferVersion]);
	const purgeSessionEvents = useCallback((sessionId: string) => {
		bufferRef.current.purgeSession(sessionId);
		frameBatchRef.current?.purgeSession(sessionId);
		setBufferVersion((version) => version + 1);
	}, []);
	const purgeSessionsEvents = useCallback((sessionIds: Iterable<string>) => {
		const ids = [...sessionIds];
		bufferRef.current.purgeSessions(ids);
		frameBatchRef.current?.purgeSessions(ids);
		setBufferVersion((version) => version + 1);
	}, []);
	const purgeThroughTurnEvents = useCallback((sessionId: string, turnId: string) => {
		bufferRef.current.purgeThroughTurn(sessionId, turnId);
		setBufferVersion((version) => version + 1);
	}, []);
	const purgeThroughSessionTerminalEvents = useCallback((sessionId: string) => {
		bufferRef.current.purgeThroughSessionTerminal(sessionId);
		setBufferVersion((version) => version + 1);
	}, []);
	const getBufferStats = useCallback(() => bufferRef.current.stats(), []);

	return {
		activityEvents,
		events,
		purgeSessionEvents,
		purgeSessionsEvents,
		purgeThroughTurnEvents,
		purgeThroughSessionTerminalEvents,
		getBufferStats,
	};
}
