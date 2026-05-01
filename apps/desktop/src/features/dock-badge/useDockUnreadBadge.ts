import { useEffect } from "react";
import type { WorkspaceSummary } from "@/features/workspaces";
import { selectUnreadSessionCount } from "./selector";

export function useDockUnreadBadge(workspaces: WorkspaceSummary[]) {
	const unreadCount = selectUnreadSessionCount(workspaces);

	useEffect(() => {
		if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
			return;
		}

		let disposed = false;

		void import("@tauri-apps/api/window")
			.then(({ getCurrentWindow }) => getCurrentWindow())
			.then(async (currentWindow) => {
				if (disposed) {
					return;
				}

				if (unreadCount > 0) {
					try {
						await currentWindow.setBadgeCount(unreadCount);
						return;
					} catch {
						/* badge count unavailable */
					}
				}

				try {
					await currentWindow.setBadgeLabel(unreadCount > 0 ? String(unreadCount) : "");
				} catch {
					/* badge label unavailable */
				}
			})
			.catch(() => {
				/* badge API unavailable */
			});

		return () => {
			disposed = true;
		};
	}, [unreadCount]);
}
