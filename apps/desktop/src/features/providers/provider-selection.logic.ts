import type { ProviderCatalog } from "@dcc/contracts";
import {
	type ProviderRegistryKey,
	resolveModelAlias,
} from "@/lib/provider-model-registry";

export const SELECTED_PROVIDER_STORAGE_KEY = "dcc.selectedProviderId";
export const SELECTED_MODEL_STORAGE_KEY = "dcc.selectedModelId";

/**
 * Per-thread composer picks keyed by conversation context.
 * Survives snapshot refreshes so the footer stays aligned when switching turns or reloading catalog.
 */
export const SESSION_COMPOSER_MAP_KEY = "dcc.sessionComposerSelections";

/**
 * Last provider/model picked per project, so a new task in a project starts
 * where that project's work left off rather than on whatever another project
 * used last.
 */
export const PROJECT_COMPOSER_MAP_KEY = "dcc.projectComposerSelections";

export type SessionComposerSelection = {
	providerId: string;
	modelId: string;
};

/**
 * A pick the person made inside a task, plus the provider the task ran on
 * when they made it. Once the task runs on another provider, the pick is
 * stale and the task's own provider wins.
 */
export type StoredSessionComposerSelection = SessionComposerSelection & {
	sessionProviderId?: string;
};

export function isProviderEnabled(
	provider: ProviderCatalog["providers"][number],
): boolean {
	return provider.enabled !== false;
}

/**
 * Switched on and healthy: the only providers the composer offers for new
 * work. A provider without a health report (fallback catalog) counts as ready.
 */
export function isProviderReady(
	provider: ProviderCatalog["providers"][number],
): boolean {
	return (
		isProviderEnabled(provider) &&
		(provider.health == null || provider.health === "Healthy")
	);
}

/** Gate used before actions that create or enqueue new work. */
export function getProviderActionBlockReason(
	provider: ProviderCatalog["providers"][number] | null,
	disabledReason: string,
	missingReason: string,
): string | null {
	if (!provider) {
		return missingReason;
	}
	return isProviderEnabled(provider) ? null : disabledReason;
}

function readSessionComposerMap(): Record<string, StoredSessionComposerSelection> {
	if (typeof window === "undefined") {
		return {};
	}
	try {
		const raw = window.localStorage.getItem(SESSION_COMPOSER_MAP_KEY);
		if (!raw) {
			return {};
		}
		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object") {
			return {};
		}
		return parsed as Record<string, StoredSessionComposerSelection>;
	} catch {
		return {};
	}
}

export function getSessionComposerSelection(
	sessionId: string,
): StoredSessionComposerSelection | null {
	const entry = readSessionComposerMap()[sessionId];
	if (
		!entry ||
		typeof entry.providerId !== "string" ||
		typeof entry.modelId !== "string"
	) {
		return null;
	}
	return entry;
}

export function setSessionComposerSelection(
	sessionId: string,
	selection: StoredSessionComposerSelection,
) {
	if (typeof window === "undefined") {
		return;
	}
	const map = readSessionComposerMap();
	map[sessionId] = selection;
	window.localStorage.setItem(SESSION_COMPOSER_MAP_KEY, JSON.stringify(map));
}

function readProjectComposerMap(): Record<string, SessionComposerSelection> {
	if (typeof window === "undefined") {
		return {};
	}
	try {
		const parsed = JSON.parse(
			window.localStorage.getItem(PROJECT_COMPOSER_MAP_KEY) ?? "{}",
		) as unknown;
		return parsed && typeof parsed === "object"
			? (parsed as Record<string, SessionComposerSelection>)
			: {};
	} catch {
		return {};
	}
}

export function getProjectComposerSelection(
	projectId: string,
): SessionComposerSelection | null {
	const entry = readProjectComposerMap()[projectId];
	if (
		!entry ||
		typeof entry.providerId !== "string" ||
		typeof entry.modelId !== "string"
	) {
		return null;
	}
	return entry;
}

export function setProjectComposerSelection(
	projectId: string,
	selection: SessionComposerSelection,
) {
	if (typeof window === "undefined") {
		return;
	}
	const map = readProjectComposerMap();
	const current = map[projectId];
	if (
		current?.providerId === selection.providerId &&
		current?.modelId === selection.modelId
	) {
		return;
	}
	map[projectId] = selection;
	try {
		window.localStorage.setItem(PROJECT_COMPOSER_MAP_KEY, JSON.stringify(map));
	} catch {
		// A full or blocked storage only loses the per-project default.
	}
}

/**
 * The provider/model a new task in this project starts on: the project's last
 * pick while that provider is ready and still has the model, else null so the
 * global choice stands.
 */
export function resolveProjectComposerSelection(
	providers: ProviderCatalog["providers"],
	stored: SessionComposerSelection | null,
): SessionComposerSelection | null {
	if (!stored) {
		return null;
	}
	const provider = providers.find((entry) => entry.id === stored.providerId);
	if (!provider || !isProviderReady(provider)) {
		return null;
	}
	const modelId = resolveSelectedModelId(provider, stored.modelId);
	return modelId ? { providerId: provider.id, modelId } : null;
}

/**
 * The provider/model a task's composer shows: the person's own pick while the
 * task still runs on the provider it was made on, else what the task runs on.
 * Picks saved without that provider (older builds) cannot be trusted: they
 * were often written with the previous task's model while switching tasks.
 */
export function resolveSessionComposerSelection(
	providers: ProviderCatalog["providers"],
	stored: StoredSessionComposerSelection | null,
	session: { providerId: string | null; model: string | null },
): SessionComposerSelection | null {
	if (stored && session.providerId && stored.sessionProviderId === session.providerId) {
		const provider = providers.find((entry) => entry.id === stored.providerId);
		const modelId = resolveSelectedModelId(provider ?? null, stored.modelId);
		if (provider && modelId) {
			return { providerId: provider.id, modelId };
		}
	}
	if (!session.providerId) {
		return null;
	}
	const provider = providers.find((entry) => entry.id === session.providerId);
	// A task started without an explicit model runs on the provider's default.
	const modelId = resolveSelectedModelId(provider ?? null, session.model);
	return provider && modelId ? { providerId: provider.id, modelId } : null;
}

export function resolveSelectedProviderId(
	providers: ProviderCatalog["providers"],
	storedProviderId: string | null,
) {
	if (storedProviderId) {
		const match = providers.find((provider) => provider.id === storedProviderId);
		if (match) {
			return match.id;
		}
	}

	// New work defaults to a provider that can actually start; only when none
	// is ready does it fall back to any switched-on one.
	const readyProviders = providers.filter(isProviderReady);
	const selectableProviders =
		readyProviders.length > 0 ? readyProviders : providers.filter(isProviderEnabled);
	const stableProvider = selectableProviders.find((provider) => provider.stable);
	if (stableProvider) {
		return stableProvider.id;
	}

	return selectableProviders[0]?.id ?? null;
}

export function resolveSelectedModelId(
	provider: ProviderCatalog["providers"][number] | null,
	storedModelId: string | null,
) {
	if (!provider) {
		return null;
	}

	if (storedModelId) {
		const canonicalId = resolveModelAlias(
			provider.id as ProviderRegistryKey,
			storedModelId,
		);
		const match = provider.models.find(
			(model) => model.id === canonicalId || model.id === storedModelId,
		);
		if (match) {
			return match.id;
		}
	}

	const recommendedModel = provider.models.find((model) => model.recommended);
	if (recommendedModel) {
		return recommendedModel.id;
	}

	return provider.models[0]?.id ?? null;
}

export function getProviderUnhealthyReason(
	provider: ProviderCatalog["providers"][number] | null,
): string | null {
	if (!provider) {
		return null;
	}

	const health = provider.health;
	if (health === "Healthy") {
		return null;
	}

	if (health && typeof health === "object" && "Unhealthy" in health) {
		return health.Unhealthy?.reason ?? "Provider is unavailable";
	}

	return null;
}

/**
 * Why a provider is not ready, Degraded included: a missing runtime reports
 * Degraded and still fails at start, so the picker treats both alike.
 */
export function getProviderSetupReason(
	provider: ProviderCatalog["providers"][number] | null,
): string | null {
	const unhealthy = getProviderUnhealthyReason(provider);
	if (unhealthy) {
		return unhealthy;
	}
	const health = provider?.health;
	if (health && typeof health === "object" && "Degraded" in health) {
		return health.Degraded?.reason ?? "Provider is unavailable";
	}
	return null;
}
