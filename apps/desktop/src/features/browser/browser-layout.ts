/** Layout policy for the browser companion surface.
 *
 * The browser is docked only when both columns can remain useful. Compact
 * windows use the existing takeover presentation, which also keeps native
 * child-WebView bounds out of the composer and its overlays.
 */
export const DEFAULT_BROWSER_SURFACE_WIDTH = 520;
export const MIN_BROWSER_SURFACE_WIDTH = 360;
export const MAX_BROWSER_SURFACE_WIDTH = 840;
export const MIN_BROWSER_CHAT_WIDTH = 520;
export const BROWSER_SPLITTER_WIDTH = 6;
export const BROWSER_SPLIT_BREAKPOINT =
	MIN_BROWSER_CHAT_WIDTH + MIN_BROWSER_SURFACE_WIDTH + BROWSER_SPLITTER_WIDTH;

const STORAGE_PREFIX = "dcc.browserSurfaceWidth.v1";

export function browserSurfaceStorageKey(workspaceId: string) {
	return `${STORAGE_PREFIX}:${encodeURIComponent(workspaceId)}`;
}

export function clampBrowserSurfaceWidth(width: number) {
	const safeWidth = Number.isFinite(width)
		? width
		: DEFAULT_BROWSER_SURFACE_WIDTH;
	return Math.min(
		MAX_BROWSER_SURFACE_WIDTH,
		Math.max(MIN_BROWSER_SURFACE_WIDTH, Math.round(safeWidth)),
	);
}

export function clampBrowserSurfaceWidthForContainer(
	width: number,
	containerWidth: number,
) {
	const safeContainerWidth = Number.isFinite(containerWidth)
		? containerWidth
		: BROWSER_SPLIT_BREAKPOINT;
	const maxForContainer = Math.min(
		MAX_BROWSER_SURFACE_WIDTH,
		Math.max(
			MIN_BROWSER_SURFACE_WIDTH,
			Math.floor(safeContainerWidth - MIN_BROWSER_CHAT_WIDTH - BROWSER_SPLITTER_WIDTH),
		),
	);
	return Math.min(maxForContainer, clampBrowserSurfaceWidth(width));
}

export function shouldSplitBrowser(containerWidth: number) {
	return Number.isFinite(containerWidth) && containerWidth >= BROWSER_SPLIT_BREAKPOINT;
}

/**
 * An expanded terminal temporarily collapses the Inspector. When Browser opens
 * from that state, retain the Inspector state from before the terminal took
 * ownership instead of treating the temporary collapse as a user preference.
 */
export function effectiveInspectorCollapsedForBrowserOpen(input: {
	inspectorCollapsed?: boolean;
	inspectorBeforeTerminalExpand: boolean | null;
}) {
	return input.inspectorBeforeTerminalExpand ?? input.inspectorCollapsed ?? true;
}

export function shouldRestoreInspectorAfterBrowserClose(input: {
	currentWorkspaceId: string;
	currentSessionId: string | null;
	currentInspectorCollapsed?: boolean;
	openedWorkspaceId: string;
	openedSessionId: string | null;
	openedCycle: number;
	currentCycle: number;
	openedInspectorCollapsed: boolean;
}) {
	return (
		input.currentWorkspaceId === input.openedWorkspaceId &&
		input.currentSessionId === input.openedSessionId &&
		input.openedCycle === input.currentCycle &&
		input.currentInspectorCollapsed === true &&
		input.openedInspectorCollapsed === false
	);
}

export function readBrowserSurfaceWidth(workspaceId: string) {
	if (typeof window === "undefined") return DEFAULT_BROWSER_SURFACE_WIDTH;
	try {
		const rawValue = window.localStorage.getItem(browserSurfaceStorageKey(workspaceId));
		if (rawValue === null) return DEFAULT_BROWSER_SURFACE_WIDTH;
		const value = Number(rawValue);
		return Number.isFinite(value)
			? clampBrowserSurfaceWidth(value)
			: DEFAULT_BROWSER_SURFACE_WIDTH;
	} catch {
		return DEFAULT_BROWSER_SURFACE_WIDTH;
	}
}

export function persistBrowserSurfaceWidth(workspaceId: string, width: number) {
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(
			browserSurfaceStorageKey(workspaceId),
			String(clampBrowserSurfaceWidth(width)),
		);
	} catch {
		// Layout preferences are progressive enhancement; never block the workbench.
	}
}
