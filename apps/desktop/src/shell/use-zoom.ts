import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 2.0;
export const ZOOM_STEP = 0.1;

export function clampZoom(value: number): number {
	if (!Number.isFinite(value)) {
		return 1;
	}

	const clamped = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, value));
	return Math.round(clamped * 100) / 100;
}

export function useZoom(zoom = 1): void {
	useEffect(() => {
		try {
			void getCurrentWebview().setZoom(clampZoom(zoom)).catch(() => {});
		} catch {
			// Outside Tauri or webview not ready yet.
		}
	}, [zoom]);
}
