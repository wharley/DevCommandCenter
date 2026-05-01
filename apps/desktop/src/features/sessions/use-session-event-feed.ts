import { useEffect, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

export function useSessionEventFeed() {
	const [events, setEvents] = useState<CoreEvent[]>([]);

	useEffect(() => {
		let disposed = false;

		void listenSessionEvents((event) => {
			if (disposed) {
				return;
			}

			setEvents((current) => [...current, event].slice(-12));
		}).then((unlisten) => {
			if (disposed) {
				void unlisten();
			}
		});

		return () => {
			disposed = true;
		};
	}, []);

	return { events };
}
