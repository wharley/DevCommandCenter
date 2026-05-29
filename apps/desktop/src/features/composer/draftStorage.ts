import { DEFAULT_EFFORT_LEVEL } from "./effort";

export type EffortSelection = {
	effort: string;
	ultrathink: boolean;
};

const DEFAULT_EFFORT_SELECTION: EffortSelection = {
	effort: DEFAULT_EFFORT_LEVEL,
	ultrathink: false,
};

export function loadEffortSelection(key: string): EffortSelection {
	if (typeof window === "undefined") {
		return DEFAULT_EFFORT_SELECTION;
	}

	const raw = window.localStorage.getItem(key);
	if (!raw) {
		return DEFAULT_EFFORT_SELECTION;
	}

	try {
		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object") {
			return DEFAULT_EFFORT_SELECTION;
		}

		const record = parsed as Record<string, unknown>;
		return {
			effort:
				typeof record.effort === "string" && record.effort.length > 0
					? record.effort
					: DEFAULT_EFFORT_LEVEL,
			ultrathink: record.ultrathink === true,
		};
	} catch {
		return DEFAULT_EFFORT_SELECTION;
	}
}

export function saveEffortSelection(key: string, selection: EffortSelection) {
	if (typeof window === "undefined") {
		return;
	}

	window.localStorage.setItem(key, JSON.stringify(selection));
}

export function loadDraft(key: string) {
	if (typeof window === "undefined") {
		return "";
	}

	return window.localStorage.getItem(key) ?? "";
}

export function saveDraft(key: string, value: string) {
	if (typeof window === "undefined") {
		return;
	}

	if (value.trim().length === 0) {
		window.localStorage.removeItem(key);
		return;
	}

	window.localStorage.setItem(key, value);
}

export function clearDraft(key: string) {
	if (typeof window === "undefined") {
		return;
	}

	window.localStorage.removeItem(key);
}
