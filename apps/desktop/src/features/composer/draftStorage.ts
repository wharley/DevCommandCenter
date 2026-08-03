import { DEFAULT_EFFORT_LEVEL } from "./effort";
import type { ProviderApprovalPolicy } from "@dcc/contracts";

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

export function loadApprovalPolicy(
	key: string,
	supportedPolicies: readonly ProviderApprovalPolicy[],
): ProviderApprovalPolicy | null {
	if (supportedPolicies.length === 0) return null;

	const fallback = supportedPolicies.includes("auto")
		? "auto"
		: (supportedPolicies[0] ?? null);
	if (typeof window === "undefined") return fallback;

	const stored = window.localStorage.getItem(key) as ProviderApprovalPolicy | null;
	return stored && supportedPolicies.includes(stored) ? stored : fallback;
}

export function saveApprovalPolicy(
	key: string,
	policy: ProviderApprovalPolicy,
) {
	if (typeof window === "undefined") return;
	window.localStorage.setItem(key, policy);
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
