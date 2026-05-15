import { useEffect, useState, useSyncExternalStore } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";
import {
	getActiveRemoteEnvironment,
	subscribeRemoteEnvironmentStore,
} from "@/features/settings/remote-environments-store";

function getSessionFeedScope() {
	const environment = getActiveRemoteEnvironment();
	if (!environment?.endpoint || !environment.bearerToken) {
		return "local";
	}
	return `remote:${environment.id}:${environment.endpoint}:${environment.bearerToken}`;
}

export function useSessionEventFeed() {
	const [events, setEvents] = useState<CoreEvent[]>([]);
	const feedScope = useSyncExternalStore(
		subscribeRemoteEnvironmentStore,
		getSessionFeedScope,
		() => "local",
	);

	useEffect(() => {
		let disposed = false;
		let cleanup: (() => void) | null = null;

		setEvents([]);

		void listenSessionEvents((event) => {
			if (disposed) {
				return;
			}

			setEvents((current) => [...current, event].slice(-12));
		}).then((unlisten) => {
			if (disposed) {
				void unlisten();
				return;
			}

			cleanup = unlisten;
		}).catch((error) => {
			if (!disposed) {
				console.error("[dcc] failed to subscribe to session events:", error);
			}
		});

		return () => {
			disposed = true;
			cleanup?.();
		};
	}, [feedScope]);

	return { events };
}
