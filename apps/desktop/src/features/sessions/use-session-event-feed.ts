import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

import { SessionEventFrameBatch } from "./session-event-frame-batch";
import {
	sessionIdForLiveEvent,
	SessionLiveEventBuffer,
} from "./session-live-event-buffer";

const MAX_ACTIVITY_EVENTS = 12;

/**
 * Subscribes to the global session event stream.
 *
 * `onEvent` fires for every event, in native arrival order, in a single React
 * batch on the next animation frame. Use it to drive per-session state (e.g.
 * snapshots) so background sessions keep updating even when their tab is not
 * selected. The returned `events` and `activityEvents` contain only the selected
 * session (plus unscoped events), preventing background deltas from rebuilding
 * the active conversation tree. All session buckets remain buffered for
 * durability until their persisted history catches up.
 */
export function useSessionEventFeed(
	onEvent?: (event: CoreEvent) => void,
	selectedSessionId: string | null = null,
) {
	const bufferRef = useRef(new SessionLiveEventBuffer());
	const [bufferVersion, setBufferVersion] = useState(0);
	const [activityEvents, setActivityEvents] = useState<CoreEvent[]>([]);
	const onEventRef = useRef(onEvent);
	const selectedSessionIdRef = useRef(selectedSessionId);
	const frameBatchRef = useRef<SessionEventFrameBatch | null>(null);
	selectedSessionIdRef.current = selectedSessionId;

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
			const activeSessionId = selectedSessionIdRef.current;
			const selectedEvents = events.filter((event) => {
				const eventSessionId = sessionIdForLiveEvent(event);
				return eventSessionId === null || eventSessionId === activeSessionId;
			});
			if (selectedEvents.length === 0) return;
			setBufferVersion((version) => version + 1);
			setActivityEvents((current) =>
				[...current, ...selectedEvents].slice(-MAX_ACTIVITY_EVENTS),
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

	useEffect(() => {
		setActivityEvents(
			bufferRef.current
				.eventsForSession(selectedSessionId)
				.slice(-MAX_ACTIVITY_EVENTS),
		);
	}, [selectedSessionId]);

	const events = useMemo(
		() => bufferRef.current.eventsForSession(selectedSessionId),
		[bufferVersion, selectedSessionId],
	);
	const purgeSessionEvents = useCallback((sessionId: string) => {
		bufferRef.current.purgeSession(sessionId);
		frameBatchRef.current?.purgeSession(sessionId);
		if (selectedSessionIdRef.current === sessionId) {
			setBufferVersion((version) => version + 1);
		}
	}, []);
	const purgeSessionsEvents = useCallback((sessionIds: Iterable<string>) => {
		const ids = [...sessionIds];
		bufferRef.current.purgeSessions(ids);
		frameBatchRef.current?.purgeSessions(ids);
		if (selectedSessionIdRef.current && ids.includes(selectedSessionIdRef.current)) {
			setBufferVersion((version) => version + 1);
		}
	}, []);
	const purgeThroughTurnEvents = useCallback((sessionId: string, turnId: string) => {
		bufferRef.current.purgeThroughTurn(sessionId, turnId);
		if (selectedSessionIdRef.current === sessionId) {
			setBufferVersion((version) => version + 1);
		}
	}, []);
	const purgeThroughSessionTerminalEvents = useCallback((sessionId: string) => {
		bufferRef.current.purgeThroughSessionTerminal(sessionId);
		if (selectedSessionIdRef.current === sessionId) {
			setBufferVersion((version) => version + 1);
		}
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
