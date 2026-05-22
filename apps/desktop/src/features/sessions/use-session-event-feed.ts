import { useEffect, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

/**
 * Subscribes to the global session event stream.
 *
 * `onEvent` fires for every event as it arrives, before the display buffer is
 * capped — use it to drive per-session state (e.g. snapshots) so background
 * sessions keep updating even when their tab is not selected. The returned
 * `events` array is the capped (last 12) buffer meant only for the activity feed.
 */
export function useSessionEventFeed(onEvent?: (event: CoreEvent) => void) {
	const [events, setEvents] = useState<CoreEvent[]>([]);
	const onEventRef = useRef(onEvent);

	useEffect(() => {
		onEventRef.current = onEvent;
	}, [onEvent]);

	useEffect(() => {
		let disposed = false;
		let cleanup: (() => void) | null = null;

		setEvents([]);

		void listenSessionEvents((event) => {
			if (disposed) {
				return;
			}
			onEventRef.current?.(event);
			setEvents((current) => [...current, event].slice(-12));
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

	return { events };
}
