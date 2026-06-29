import { useEffect, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

const MAX_PROJECTION_EVENTS = 5000;
const MAX_ACTIVITY_EVENTS = 12;

/**
 * Subscribes to the global session event stream.
 *
 * `onEvent` fires for every event as it arrives, before the display buffer is
 * capped — use it to drive per-session state (e.g. snapshots) so background
 * sessions keep updating even when their tab is not selected. The returned
 * `events` is intentionally larger than the visible activity feed because the
 * conversation projection needs every live delta until persisted history
 * catches up. `activityEvents` is the small UI-only feed.
 */
export function useSessionEventFeed(onEvent?: (event: CoreEvent) => void) {
	const [events, setEvents] = useState<CoreEvent[]>([]);
	const [activityEvents, setActivityEvents] = useState<CoreEvent[]>([]);
	const onEventRef = useRef(onEvent);

	useEffect(() => {
		onEventRef.current = onEvent;
	}, [onEvent]);

	useEffect(() => {
		let disposed = false;
		let cleanup: (() => void) | null = null;

		setEvents([]);
		setActivityEvents([]);

		void listenSessionEvents((event) => {
			if (disposed) {
				return;
			}
			onEventRef.current?.(event);
			setEvents((current) => [...current, event].slice(-MAX_PROJECTION_EVENTS));
			setActivityEvents((current) => [...current, event].slice(-MAX_ACTIVITY_EVENTS));
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
			cleanup?.();
		};
	}, []);

	return { activityEvents, events };
}
