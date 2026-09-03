import type { Capabilities, ProviderRuntimeConfig } from "@dcc/contracts";

export const PROVIDER_RUNTIME_STORAGE_KEY = "dcc.providerRuntimeConfigs";

export const SUBAGENT_CONCURRENCY_OPTIONS = [1, 2, 4, 6, 8] as const;
/** @deprecated Use SUBAGENT_CONCURRENCY_OPTIONS; the limit is capability-gated, not Codex-specific. */
export const CODEX_SUBAGENT_CONCURRENCY_OPTIONS = SUBAGENT_CONCURRENCY_OPTIONS;

/**
 * Runtime-config capabilities are declared by the backend registry and
 * projected through the catalog. The backend rejects any field an adapter
 * would ignore, so the renderer must project the same subset.
 */
export type ProviderRuntimeCapabilities = Pick<
	Capabilities,
	| "supportsRuntimeHome"
	| "supportsRuntimeBinary"
	| "supportsShadowHome"
	| "supportsSubagentConcurrency"
>;

export type ProviderRuntimeDraft = {
	binaryPath: string;
	homePath: string;
	shadowHomePath: string;
	maxConcurrentSubagents: string;
};

export type ProviderRuntimeSettings = Record<string, ProviderRuntimeDraft>;

const EMPTY_DRAFT: ProviderRuntimeDraft = {
	binaryPath: "",
	homePath: "",
	shadowHomePath: "",
	maxConcurrentSubagents: "",
};

function normalizeMaxConcurrentSubagents(value: unknown): string {
	const normalized = typeof value === "number" ? String(value) : value;
	return typeof normalized === "string" &&
		SUBAGENT_CONCURRENCY_OPTIONS.some(
			(option) => String(option) === normalized,
		)
		? normalized
		: "";
}

function normalizeDraftEntry(value: unknown): ProviderRuntimeDraft | null {
	if (!value || typeof value !== "object") {
		return null;
	}

	const record = value as Record<string, unknown>;
	return {
		binaryPath: typeof record.binaryPath === "string" ? record.binaryPath : "",
		homePath: typeof record.homePath === "string" ? record.homePath : "",
		shadowHomePath:
			typeof record.shadowHomePath === "string" ? record.shadowHomePath : "",
		maxConcurrentSubagents: normalizeMaxConcurrentSubagents(
			record.maxConcurrentSubagents,
		),
	};
}

export function supportsProviderRuntimeHome(
	capabilities: ProviderRuntimeCapabilities | null | undefined,
): boolean {
	return capabilities?.supportsRuntimeHome === true;
}

export function supportsProviderRuntimeBinary(
	capabilities: ProviderRuntimeCapabilities | null | undefined,
): boolean {
	return capabilities?.supportsRuntimeBinary === true;
}

export function supportsProviderShadowHome(
	capabilities: ProviderRuntimeCapabilities | null | undefined,
): boolean {
	return capabilities?.supportsShadowHome === true;
}

export function supportsProviderSubagentConcurrency(
	capabilities: ProviderRuntimeCapabilities | null | undefined,
): boolean {
	return capabilities?.supportsSubagentConcurrency === true;
}

/** A provider exposes runtime settings when it honors at least one field. */
export function supportsProviderRuntime(
	capabilities: ProviderRuntimeCapabilities | null | undefined,
): boolean {
	return (
		supportsProviderRuntimeBinary(capabilities) ||
		supportsProviderRuntimeHome(capabilities) ||
		supportsProviderShadowHome(capabilities) ||
		supportsProviderSubagentConcurrency(capabilities)
	);
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
			binaryPath: draft.binaryPath,
			homePath: draft.homePath,
			shadowHomePath: draft.shadowHomePath,
			maxConcurrentSubagents: draft.maxConcurrentSubagents,
		},
	};

	if (
		!draft.binaryPath.trim() &&
		!draft.homePath.trim() &&
		!draft.shadowHomePath.trim() &&
		!draft.maxConcurrentSubagents
	) {
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

/**
 * Projects a persisted draft into the session runtime config. When the
 * provider capabilities are known, fields the adapter does not honor are
 * dropped here so a stale local draft never reaches the backend authority,
 * which rejects them.
 */
export function draftToProviderRuntimeConfig(
	draft: ProviderRuntimeDraft | null | undefined,
	capabilities?: ProviderRuntimeCapabilities | null,
): ProviderRuntimeConfig | null {
	if (!draft) {
		return null;
	}

	const gated = capabilities !== undefined;
	const binaryPath =
		!gated || supportsProviderRuntimeBinary(capabilities)
			? draft.binaryPath.trim()
			: "";
	const homePath =
		!gated || supportsProviderRuntimeHome(capabilities) ? draft.homePath.trim() : "";
	const shadowHomePath =
		!gated || supportsProviderShadowHome(capabilities)
			? draft.shadowHomePath.trim()
			: "";
	const maxConcurrentSubagents =
		!gated || supportsProviderSubagentConcurrency(capabilities)
			? normalizeMaxConcurrentSubagents(draft.maxConcurrentSubagents)
			: "";

	if (!binaryPath && !homePath && !shadowHomePath && !maxConcurrentSubagents) {
		return null;
	}

	return {
		binaryPath: binaryPath || null,
		homePath: homePath || null,
		shadowHomePath: shadowHomePath || null,
		maxConcurrentSubagents: maxConcurrentSubagents
			? Number(maxConcurrentSubagents)
			: null,
	};
}
