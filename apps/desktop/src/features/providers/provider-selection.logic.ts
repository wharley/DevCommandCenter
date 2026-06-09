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

export type SessionComposerSelection = {
	providerId: string;
	modelId: string;
};

function readSessionComposerMap(): Record<string, SessionComposerSelection> {
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
		return parsed as Record<string, SessionComposerSelection>;
	} catch {
		return {};
	}
}

export function getSessionComposerSelection(
	sessionId: string,
): SessionComposerSelection | null {
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
	selection: SessionComposerSelection,
) {
	if (typeof window === "undefined") {
		return;
	}
	const map = readSessionComposerMap();
	map[sessionId] = selection;
	window.localStorage.setItem(SESSION_COMPOSER_MAP_KEY, JSON.stringify(map));
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

	const stableProvider = providers.find((provider) => provider.stable);
	if (stableProvider) {
		return stableProvider.id;
	}

	return providers[0]?.id ?? null;
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
