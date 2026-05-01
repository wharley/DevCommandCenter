export const SIDEBAR_WIDTH_STORAGE_KEY = "dcc.shell.sidebarWidth";
export const INSPECTOR_WIDTH_STORAGE_KEY = "dcc.shell.inspectorWidth";
export const PREFERRED_EDITOR_STORAGE_KEY = "dcc.shell.preferredEditor";

export const DEFAULT_SIDEBAR_WIDTH = 336;
export const DEFAULT_INSPECTOR_WIDTH = 360;
export const MIN_SIDEBAR_WIDTH = 240;
export const MAX_SIDEBAR_WIDTH = 520;
export const MIN_INSPECTOR_WIDTH = 280;
export const MAX_INSPECTOR_WIDTH = 560;
export const SIDEBAR_RESIZE_STEP = 16;
export const SIDEBAR_RESIZE_HIT_AREA = 20;

export function clampSidebarWidth(width: number) {
	return Math.min(MAX_SIDEBAR_WIDTH, Math.max(MIN_SIDEBAR_WIDTH, width));
}

export function clampInspectorWidth(width: number) {
	return Math.min(MAX_INSPECTOR_WIDTH, Math.max(MIN_INSPECTOR_WIDTH, width));
}

function readStoredWidth(storageKey: string, fallback: number) {
	if (typeof window === "undefined") {
		return fallback;
	}

	const stored = window.localStorage.getItem(storageKey);
	if (!stored) {
		return fallback;
	}

	const parsed = Number.parseInt(stored, 10);
	return Number.isFinite(parsed) ? parsed : fallback;
}

export function getInitialSidebarWidth(storageKey = SIDEBAR_WIDTH_STORAGE_KEY) {
	return clampSidebarWidth(readStoredWidth(storageKey, DEFAULT_SIDEBAR_WIDTH));
}

export function getInitialInspectorWidth(
	storageKey = INSPECTOR_WIDTH_STORAGE_KEY,
) {
	return clampInspectorWidth(readStoredWidth(storageKey, DEFAULT_INSPECTOR_WIDTH));
}
