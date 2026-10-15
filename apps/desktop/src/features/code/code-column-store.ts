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
