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
	visible: boolean;
	url: string | null;
	title: string | null;
};

export function openBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	initialUrl?: string | null;
	bounds: BrowserBounds;
}) {
	return invoke<BrowserSnapshot>("browser_open", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		initialUrl: input.initialUrl ?? null,
		bounds: input.bounds,
	});
}

export function navigateBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
	url: string;
}) {
	return invoke<BrowserSnapshot>("browser_navigate", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		url: input.url,
	});
}

export function reloadBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
}) {
	return invoke<BrowserSnapshot>("browser_reload", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
	});
}

export function setBrowserBounds(input: {
	workspaceId: string;
	sessionId: string | null;
	bounds: BrowserBounds;
}) {
	return invoke<void>("browser_set_bounds", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
		bounds: input.bounds,
	});
}

export function hideBrowser(input: {
	workspaceId: string;
	sessionId: string | null;
}) {
	return invoke<BrowserSnapshot>("browser_hide", {
		workspaceId: input.workspaceId,
		sessionId: input.sessionId,
	});
}

export function listenBrowserState(
	onState: (snapshot: BrowserSnapshot) => void,
): Promise<UnlistenFn> {
	return listen<BrowserSnapshot>("browser://state-changed", (event) => {
		onState(event.payload);
	});
}
