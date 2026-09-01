import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type BrowserBounds = {
	x: number;
	y: number;
	width: number;
	height: number;
};

export type BrowserSnapshot = {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	visible: boolean;
	url: string | null;
	title: string | null;
};

/** Bounded page context obtained by the backend after an explicit user action. */
export type BrowserAgentContext = {
	workspaceId: string;
	sessionId: string | null;
	url: string;
	title: string | null;
	text: string;
	selectionOnly: boolean;
	truncated: boolean;
	semanticMap: BrowserSemanticMap;
};

/** Opaque, page-local references; these are not DOM selectors or automation locators. */
export type BrowserSemanticMap = {
	mapId: string;
	generation: number;
	items: BrowserSemanticItem[];
	truncated: boolean;
};

export type BrowserSemanticItem = {
	reference: string;
	role: string;
	level?: number;
	name: string;
	destination?: string;
	disabled?: boolean;
	checked?: boolean;
	selected?: boolean;
	expanded?: boolean;
	pressed?: boolean;
};

export function openBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	initialUrl?: string | null;
	bounds: BrowserBounds;
	initialOccluded?: boolean;
}) {
	return invoke<BrowserSnapshot>("browser_open", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		initialUrl: input.initialUrl ?? null,
		bounds: input.bounds,
		...(input.initialOccluded ? { initialOccluded: true } : {}),
	});
}

export function navigateBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	url: string;
}) {
	return invoke<BrowserSnapshot>("browser_navigate", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		url: input.url,
	});
}

export function reloadBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserSnapshot>("browser_reload", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

export function extractBrowserContext(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserAgentContext>("browser_extract_context", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

export function setBrowserBounds(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	bounds: BrowserBounds;
}) {
	return invoke<void>("browser_set_bounds", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		bounds: input.bounds,
	});
}

export function hideBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
}) {
	return invoke<BrowserSnapshot>("browser_hide", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
	});
}

/** Temporarily occludes the native child view while a marked DCC portal overlaps it. */
export function setBrowserOccluded(input: {
	workspaceId: string;
	sessionId: string | null;
	lifecycleToken: number;
	occluded: boolean;
	bounds?: BrowserBounds;
}) {
	return invoke<BrowserSnapshot>("browser_set_occluded", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		lifecycleToken: input.lifecycleToken,
		occluded: input.occluded,
		...(input.bounds ? { bounds: input.bounds } : {}),
	});
}

export function listenBrowserState(
	onState: (snapshot: BrowserSnapshot) => void,
): Promise<UnlistenFn> {
	return listen<BrowserSnapshot>("browser://state-changed", (event) => {
		onState(event.payload);
	});
}
