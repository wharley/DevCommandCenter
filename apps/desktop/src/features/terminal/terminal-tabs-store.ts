import { useSyncExternalStore } from "react";
import { killTerminal } from "./terminal-store";

export type TerminalTab = {
	id: string;
	title: string;
};

export type ProjectTerminals = {
	tabs: TerminalTab[];
	activeId: string | null;
};

const STORAGE_KEY = "dcc-terminal-tabs-v1";
/** Soft guard so a project does not accumulate runaway PTYs. */
export const MAX_TERMINAL_TABS = 8;

const EMPTY: ProjectTerminals = { tabs: [], activeId: null };

const state = new Map<string, ProjectTerminals>();
const listeners = new Set<() => void>();
const snapshotCache = new Map<string, ProjectTerminals>();
let hydrated = false;

function newId(): string {
	if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
		return crypto.randomUUID();
	}
	return `term-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function hydrate() {
	if (hydrated) {
		return;
	}
	hydrated = true;
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) {
			return;
		}
		const parsed = JSON.parse(raw) as Record<string, ProjectTerminals>;
		for (const [projectKey, value] of Object.entries(parsed)) {
			if (value && Array.isArray(value.tabs)) {
				state.set(projectKey, {
					tabs: value.tabs.filter((tab) => tab && tab.id),
					activeId: value.activeId ?? value.tabs[0]?.id ?? null,
				});
			}
		}
	} catch {
		/* ignore malformed persistence */
	}
}

function persist() {
	try {
		const plain: Record<string, ProjectTerminals> = {};
		for (const [projectKey, value] of state.entries()) {
			if (value.tabs.length > 0) {
				plain[projectKey] = value;
			}
		}
		localStorage.setItem(STORAGE_KEY, JSON.stringify(plain));
	} catch {
		/* ignore quota / serialization errors */
	}
}

function emit() {
	for (const listener of listeners) {
		listener();
	}
}

function read(scopeKey: string): ProjectTerminals {
	hydrate();
	return state.get(scopeKey) ?? EMPTY;
}

/** Stable snapshot per scope so `useSyncExternalStore` does not loop. */
function cachedSnapshot(scopeKey: string): ProjectTerminals {
	const current = read(scopeKey);
	const cached = snapshotCache.get(scopeKey);
	if (
		cached &&
		cached.activeId === current.activeId &&
		cached.tabs === current.tabs
	) {
		return cached;
	}
	snapshotCache.set(scopeKey, current);
	return current;
}

function write(scopeKey: string, next: ProjectTerminals) {
	state.set(scopeKey, next);
	persist();
	emit();
}

function nextTitle(tabs: TerminalTab[]): string {
	const used = new Set(tabs.map((tab) => tab.title));
	for (let i = 1; i <= MAX_TERMINAL_TABS + 1; i += 1) {
		const candidate = `Terminal ${i}`;
		if (!used.has(candidate)) {
			return candidate;
		}
	}
	return `Terminal ${tabs.length + 1}`;
}

export function getTerminalRuntimeId(scopeKey: string, terminalId: string) {
	return `${scopeKey}:${terminalId}`;
}

export function addTerminal(scopeKey: string): string | null {
	const current = read(scopeKey);
	if (current.tabs.length >= MAX_TERMINAL_TABS) {
		// At the soft cap: focus the last tab instead of spawning another PTY.
		const last = current.tabs[current.tabs.length - 1];
		if (last && current.activeId !== last.id) {
			write(scopeKey, { ...current, activeId: last.id });
		}
		return last?.id ?? null;
	}

	const tab: TerminalTab = { id: newId(), title: nextTitle(current.tabs) };
	write(scopeKey, {
		tabs: [...current.tabs, tab],
		activeId: tab.id,
	});
	return tab.id;
}

/** Ensure at least one tab exists; returns the active id. */
export function ensureTerminal(scopeKey: string): string | null {
	const current = read(scopeKey);
	if (current.tabs.length === 0) {
		return addTerminal(scopeKey);
	}
	if (!current.activeId) {
		write(scopeKey, { ...current, activeId: current.tabs[0].id });
		return current.tabs[0].id;
	}
	return current.activeId;
}

export function removeTerminal(scopeKey: string, terminalId: string) {
	const current = read(scopeKey);
	const index = current.tabs.findIndex((tab) => tab.id === terminalId);
	if (index === -1) {
		return;
	}

	killTerminal(getTerminalRuntimeId(scopeKey, terminalId));
	const tabs = current.tabs.filter((tab) => tab.id !== terminalId);

	let activeId = current.activeId;
	if (activeId === terminalId) {
		const fallback = tabs[index] ?? tabs[index - 1] ?? tabs[0] ?? null;
		activeId = fallback?.id ?? null;
	}

	write(scopeKey, { tabs, activeId });
}

export function setActiveTerminal(scopeKey: string, terminalId: string) {
	const current = read(scopeKey);
	if (current.activeId === terminalId) {
		return;
	}
	if (!current.tabs.some((tab) => tab.id === terminalId)) {
		return;
	}
	write(scopeKey, { ...current, activeId: terminalId });
}

function subscribe(listener: () => void): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

export function useProjectTerminals(scopeKey: string): ProjectTerminals {
	return useSyncExternalStore(
		subscribe,
		() => cachedSnapshot(scopeKey),
		() => cachedSnapshot(scopeKey),
	);
}
