import type { ProviderRuntimeConfig } from "@dcc/contracts";

export const PROVIDER_RUNTIME_STORAGE_KEY = "dcc.providerRuntimeConfigs";

export const PROVIDER_RUNTIME_SUPPORTED_IDS = new Set([
	"claude_code",
	"codex",
	"gemini",
]);

export type ProviderRuntimeDraft = {
	homePath: string;
	shadowHomePath: string;
};

export type ProviderRuntimeSettings = Record<string, ProviderRuntimeDraft>;

const EMPTY_DRAFT: ProviderRuntimeDraft = {
	homePath: "",
	shadowHomePath: "",
};

function normalizeDraftEntry(value: unknown): ProviderRuntimeDraft | null {
	if (!value || typeof value !== "object") {
		return null;
	}

	const record = value as Record<string, unknown>;
	return {
		homePath: typeof record.homePath === "string" ? record.homePath : "",
		shadowHomePath:
			typeof record.shadowHomePath === "string" ? record.shadowHomePath : "",
	};
}

export function supportsProviderRuntime(providerId: string): boolean {
	return PROVIDER_RUNTIME_SUPPORTED_IDS.has(providerId);
}

export function readProviderRuntimeSettings(): ProviderRuntimeSettings {
	if (typeof window === "undefined") {
		return {};
	}

	try {
		const raw = window.localStorage.getItem(PROVIDER_RUNTIME_STORAGE_KEY);
		if (!raw) {
			return {};
		}

		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object") {
			return {};
		}

		const next: ProviderRuntimeSettings = {};
		for (const [providerId, value] of Object.entries(
			parsed as Record<string, unknown>,
		)) {
			const draft = normalizeDraftEntry(value);
			if (draft) {
				next[providerId] = draft;
			}
		}

		return next;
	} catch {
		return {};
	}
}

export function writeProviderRuntimeSettings(settings: ProviderRuntimeSettings) {
	if (typeof window === "undefined") {
		return;
	}

	window.localStorage.setItem(
		PROVIDER_RUNTIME_STORAGE_KEY,
		JSON.stringify(settings),
	);
}

export function getProviderRuntimeDraft(
	settings: ProviderRuntimeSettings,
	providerId: string,
): ProviderRuntimeDraft {
	return settings[providerId] ?? EMPTY_DRAFT;
}

export function setProviderRuntimeDraft(
	settings: ProviderRuntimeSettings,
	providerId: string,
	draft: ProviderRuntimeDraft,
): ProviderRuntimeSettings {
	const next = {
		...settings,
		[providerId]: {
			homePath: draft.homePath,
			shadowHomePath: draft.shadowHomePath,
		},
	};

	if (!draft.homePath.trim() && !draft.shadowHomePath.trim()) {
		delete next[providerId];
	}

	return next;
}

export function clearProviderRuntimeDraft(
	settings: ProviderRuntimeSettings,
	providerId: string,
): ProviderRuntimeSettings {
	if (!(providerId in settings)) {
		return settings;
	}

	const next = { ...settings };
	delete next[providerId];
	return next;
}

export function draftToProviderRuntimeConfig(
	draft: ProviderRuntimeDraft | null | undefined,
): ProviderRuntimeConfig | null {
	if (!draft) {
		return null;
	}

	const homePath = draft.homePath.trim();
	const shadowHomePath = draft.shadowHomePath.trim();

	if (!homePath && !shadowHomePath) {
		return null;
	}

	return {
		homePath: homePath || null,
		shadowHomePath: shadowHomePath || null,
	};
}

