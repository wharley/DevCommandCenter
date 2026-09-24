import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { isAppshotsDesktop } from "@/lib/appshots-api";
import type { QuickLaunch } from "./api";

export function useQuickComposerNavigation(
	onOpen: (launch: QuickLaunch | null) => void,
) {
	const callback = useRef(onOpen);
	callback.current = onOpen;
	useEffect(() => {
		if (!isAppshotsDesktop()) return;
		let disposed = false;
		const subscription = listen<QuickLaunch | null>(
			"quick-composer-open-task",
			(event) => callback.current(event.payload),
		);
		void subscription
			.then((stop) => {
				if (disposed) stop();
			})
			.catch(() => {});
		return () => {
			disposed = true;
			void subscription.then((stop) => stop()).catch(() => {});
		};
	}, []);
}
