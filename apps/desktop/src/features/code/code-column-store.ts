import { useSyncExternalStore } from "react";

const STORAGE_KEY = "dcc.codeColumnOpen";
const listeners = new Set<() => void>();

function readStored(): boolean {
	try {
		return window.localStorage.getItem(STORAGE_KEY) === "true";
	} catch {
		return false;
	}
}

let open = typeof window === "undefined" ? false : readStored();

export type CodeColumnTab = "files" | "changes";
const TAB_STORAGE_KEY = "dcc.codeColumnTab";
const tabListeners = new Set<() => void>();

function readStoredTab(): CodeColumnTab {
	try {
		return window.localStorage.getItem(TAB_STORAGE_KEY) === "changes" ? "changes" : "files";
	} catch {
		return "files";
	}
}

let tab: CodeColumnTab = typeof window === "undefined" ? "files" : readStoredTab();

export function setCodeColumnTab(next: CodeColumnTab) {
	if (tab === next) return;
	tab = next;
	try {
		window.localStorage.setItem(TAB_STORAGE_KEY, next);
	} catch {
		// A layout preference; never block the workbench.
	}
	for (const listener of tabListeners) listener();
}

function subscribeTab(listener: () => void) {
	tabListeners.add(listener);
	return () => {
		tabListeners.delete(listener);
	};
}

/** Which list the code column shows (files by default). */
export function useCodeColumnTab(): CodeColumnTab {
	return useSyncExternalStore(subscribeTab, () => tab, () => "files");
}

/** Opens the code column on one of its lists. */
export function showCodeColumn(next: CodeColumnTab) {
	setCodeColumnTab(next);
	setCodeColumnOpen(true);
}

export function setCodeColumnOpen(next: boolean) {
	if (open === next) return;
	open = next;
	try {
		window.localStorage.setItem(STORAGE_KEY, String(next));
	} catch {
		// A layout preference; never block the workbench.
	}
	for (const listener of listeners) listener();
}

export function toggleCodeColumn() {
	setCodeColumnOpen(!open);
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

/** Whether the code column next to the sidebar is open (closed by default). */
export function useCodeColumnOpen(): boolean {
	return useSyncExternalStore(subscribe, () => open, () => false);
}
