import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { isAppshotsDesktop } from "@/lib/appshots-api";

type TaskNavigation = { workspaceId: string | null; sessionId: string | null };

export function useQuickComposerNavigation(
	onOpen: (launch: TaskNavigation | null) => void,
) {
	const callback = useRef(onOpen);
	callback.current = onOpen;
	useEffect(() => {
		if (!isAppshotsDesktop()) return;
		let disposed = false;
		const subscriptions = [
			listen<TaskNavigation | null>("quick-composer-open-task", (event) =>
				callback.current(event.payload),
			),
			listen<TaskNavigation | null>("menu-bar-open-task", (event) =>
				callback.current(event.payload),
			),
		];
		for (const subscription of subscriptions) {
			void subscription.then((stop) => { if (disposed) stop(); }).catch(() => {});
		}
		return () => {
			disposed = true;
			for (const subscription of subscriptions) {
				void subscription.then((stop) => stop()).catch(() => {});
			}
		};
	}, []);
}
