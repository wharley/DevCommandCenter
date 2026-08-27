/** Default width for the desktop-only companion surface beside the conversation. */
export const DEFAULT_SECONDARY_SURFACE_WIDTH = 560;
export const MIN_SECONDARY_SURFACE_WIDTH = 360;
export const MAX_SECONDARY_SURFACE_WIDTH = 840;
export const MIN_PRIMARY_SURFACE_WIDTH = 400;

const STORAGE_PREFIX = "dcc.secondarySurfaceWidth.v1";
const SELECTION_STORAGE_PREFIX = "dcc.secondarySurfaceSelection.v1";

/**
 * Only restore a surface that has no document, diff, or conflict payload to go
 * stale. File, diff, merge, and mission-spec selections are deliberately
 * session-scoped and must be opened explicitly again.
 */
export type RestorableSecondarySurfaceSelection = "plan";

export type SecondarySurfaceRestorationDecision =
	| "wait"
	| "none"
	| RestorableSecondarySurfaceSelection;

export function secondarySurfaceStorageKey(workspaceId: string) {
	return `${STORAGE_PREFIX}:${encodeURIComponent(workspaceId)}`;
}

export function secondarySurfaceSelectionStorageKey(workspaceId: string) {
	return `${SELECTION_STORAGE_PREFIX}:${encodeURIComponent(workspaceId)}`;
}

export function clampSecondarySurfaceWidth(width: number) {
	const safeWidth = Number.isFinite(width) ? width : DEFAULT_SECONDARY_SURFACE_WIDTH;
	return Math.min(
		MAX_SECONDARY_SURFACE_WIDTH,
		Math.max(MIN_SECONDARY_SURFACE_WIDTH, Math.round(safeWidth)),
	);
}

/**
 * Limits the companion surface to leave a readable conversation column. This
 * only applies while the surface is docked; compact layouts use an overlay.
 */
export function clampSecondarySurfaceWidthForContainer(
	width: number,
	containerWidth: number,
) {
	const maxForContainer = Math.min(
		MAX_SECONDARY_SURFACE_WIDTH,
		Math.max(
			MIN_SECONDARY_SURFACE_WIDTH,
			Math.floor(containerWidth - MIN_PRIMARY_SURFACE_WIDTH),
		),
	);
	return Math.min(maxForContainer, clampSecondarySurfaceWidth(width));
}

export function canDockSecondarySurface(containerWidth: number) {
	return containerWidth >= MIN_PRIMARY_SURFACE_WIDTH + MIN_SECONDARY_SURFACE_WIDTH;
}

export function readSecondarySurfaceWidth(workspaceId: string) {
	if (typeof window === "undefined") return DEFAULT_SECONDARY_SURFACE_WIDTH;
	try {
		const rawValue = window.localStorage.getItem(secondarySurfaceStorageKey(workspaceId));
		if (rawValue === null) return DEFAULT_SECONDARY_SURFACE_WIDTH;
		const value = Number(rawValue);
		return Number.isFinite(value)
			? clampSecondarySurfaceWidth(value)
			: DEFAULT_SECONDARY_SURFACE_WIDTH;
	} catch {
		return DEFAULT_SECONDARY_SURFACE_WIDTH;
	}
}

export function persistSecondarySurfaceWidth(workspaceId: string, width: number) {
	if (typeof window === "undefined") return;
	try {
		window.localStorage.setItem(
			secondarySurfaceStorageKey(workspaceId),
			String(clampSecondarySurfaceWidth(width)),
		);
	} catch {
		// Layout preferences are progressive enhancement; never block the workbench.
	}
}

export function readRestorableSecondarySurfaceSelection(
	workspaceId: string,
): RestorableSecondarySurfaceSelection | null {
	if (typeof window === "undefined") return null;
	try {
		return window.localStorage.getItem(
			secondarySurfaceSelectionStorageKey(workspaceId),
		) === "plan"
			? "plan"
			: null;
	} catch {
		return null;
	}
}

export function persistRestorableSecondarySurfaceSelection(
	workspaceId: string,
	selection: RestorableSecondarySurfaceSelection | null,
) {
	if (typeof window === "undefined") return;
	try {
		const key = secondarySurfaceSelectionStorageKey(workspaceId);
		if (selection) {
			window.localStorage.setItem(key, selection);
		} else {
			window.localStorage.removeItem(key);
		}
	} catch {
		// Layout preferences are progressive enhancement; never block the workbench.
	}
}

/**
 * Wait for App to clear a surface that belongs to the previous workspace before
 * restoring this workspace's preference. This prevents an A -> B transition
 * from treating A's open plan as if it were already B's selection.
 */
export function resolveSecondarySurfaceRestoration({
	workspaceId,
	restoredWorkspaceId,
	surfaceWorkspaceId,
	hasSurfaceSelection,
	storedSelection,
}: {
	workspaceId: string;
	restoredWorkspaceId: string | null;
	surfaceWorkspaceId: string | null;
	hasSurfaceSelection: boolean;
	storedSelection: RestorableSecondarySurfaceSelection | null;
}): SecondarySurfaceRestorationDecision {
	if (restoredWorkspaceId === workspaceId) return "none";
	if (hasSurfaceSelection && surfaceWorkspaceId !== workspaceId) return "wait";
	if (hasSurfaceSelection) return "none";
	return storedSelection ?? "none";
}
