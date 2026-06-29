export const DEFAULT_SIDEBAR_WIDTH = 336;
export const MIN_SIDEBAR_WIDTH = 220;
export const MAX_SIDEBAR_WIDTH = 520;
export const DEFAULT_INSPECTOR_WIDTH = 360;
export const MIN_INSPECTOR_WIDTH = 280;
export const MAX_INSPECTOR_WIDTH = 560;
export const SIDEBAR_RESIZE_STEP = 16;
export const SIDEBAR_RESIZE_HIT_AREA = 20;

export const SIDEBAR_WIDTH_STORAGE_KEY = "dcc.workspaceSidebarWidth";
export const INSPECTOR_WIDTH_STORAGE_KEY = "dcc.workspaceInspectorWidth";
export const SIDEBAR_COLLAPSED_STORAGE_KEY = "dcc.workspaceSidebarCollapsed";
export const INSPECTOR_COLLAPSED_STORAGE_KEY = "dcc.workspaceInspectorCollapsed";

export const INSPECTOR_SECTION_HEADER_CLASS =
	"flex h-8 min-w-0 shrink-0 items-center justify-between border-b border-border/60 bg-muted/25 px-3";
export const INSPECTOR_SECTION_TITLE_CLASS =
	"text-[13px] leading-8 font-medium tracking-[-0.01em] text-muted-foreground";
export const INSPECTOR_TAB_BUTTON_CLASS =
	"relative inline-flex h-full cursor-pointer items-center justify-center gap-1.5 px-3 text-[12px] font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-0";

function clamp(width: number, min: number, max: number) {
	return Math.min(max, Math.max(min, width));
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

export function clampSidebarWidth(width: number) {
	return clamp(width, MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
}

export function clampInspectorWidth(width: number) {
	return clamp(width, MIN_INSPECTOR_WIDTH, MAX_INSPECTOR_WIDTH);
}

export function getInitialSidebarWidth(storageKey = SIDEBAR_WIDTH_STORAGE_KEY) {
	return clampSidebarWidth(readStoredWidth(storageKey, DEFAULT_SIDEBAR_WIDTH));
}

export function getInitialInspectorWidth(
	storageKey = INSPECTOR_WIDTH_STORAGE_KEY,
) {
	return clampInspectorWidth(readStoredWidth(storageKey, DEFAULT_INSPECTOR_WIDTH));
}

export function getInitialCollapsed(
	storageKey: string,
	fallback = false,
) {
	if (typeof window === "undefined") {
		return fallback;
	}

	const stored = window.localStorage.getItem(storageKey);
	if (stored === null) {
		return fallback;
	}

	return stored === "true";
}

/**
 * Inspector starts collapsed ("calm" default) for brand-new installs, but keeps the
 * dense layout for anyone who already used the app, so we never yank the inspector
 * out from under an existing user. Detection: any persisted `dcc.*` layout pref means
 * this isn't a first run. Once the user explicitly toggles, the stored value wins.
 */
export function getInitialInspectorCollapsed() {
	if (typeof window === "undefined") {
		return true;
	}

	const stored = window.localStorage.getItem(INSPECTOR_COLLAPSED_STORAGE_KEY);
	if (stored !== null) {
		return stored === "true";
	}

	const isExistingInstall =
		window.localStorage.getItem(SIDEBAR_WIDTH_STORAGE_KEY) !== null ||
		window.localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) !== null ||
		window.localStorage.getItem(INSPECTOR_WIDTH_STORAGE_KEY) !== null;

	return !isExistingInstall;
}
