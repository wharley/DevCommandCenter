import { useSyncExternalStore } from "react";
import type { ProviderCatalog } from "@dcc/contracts";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";

export type ModelFavorite = {
	providerId: string;
	modelId: string;
	/** null means the model manages effort itself. */
	effort: string | null;
};

export const MODEL_FAVORITES_STORAGE_KEY = "dcc.composer.model-favorites.v1";

export function modelFavoriteKey(favorite: ModelFavorite) {
	return JSON.stringify([favorite.providerId, favorite.modelId, favorite.effort]);
}

export function parseModelFavorites(raw: string | null): ModelFavorite[] {
	try {
		const parsed: unknown = JSON.parse(raw ?? "[]");
		if (!Array.isArray(parsed)) return [];
		const seen = new Set<string>();
		return parsed.filter((entry): entry is ModelFavorite => {
			if (
				!entry ||
				typeof entry.providerId !== "string" || !entry.providerId.trim() ||
				typeof entry.modelId !== "string" || !entry.modelId.trim() ||
				(entry.effort !== null &&
					(typeof entry.effort !== "string" || !entry.effort.trim()))
			) return false;
			const key = modelFavoriteKey(entry);
			if (seen.has(key)) return false;
			seen.add(key);
			return true;
		});
	} catch {
		return [];
	}
}

export function resolveModelFavorite(
	favorite: ModelFavorite,
	providers: ProviderCatalog["providers"],
) {
	const provider = providers.find((entry) => entry.id === favorite.providerId);
	const model = provider?.models.find((entry) => entry.id === favorite.modelId);
	const supported = Boolean(model && (favorite.effort === null
		? model.effortLevels.length === 0
		: model.effortLevels.includes(favorite.effort) ||
			(favorite.effort === "ultrathink" && model.effortLevels.length > 0)));
	return {
		provider,
		model,
		available: Boolean(provider && isProviderEnabled(provider) && supported),
	};
}

let snapshot: ModelFavorite[] | undefined;
const listeners = new Set<() => void>();

export function getModelFavorites(): ModelFavorite[] {
	if (!snapshot) {
		try {
			snapshot = parseModelFavorites(window.localStorage.getItem(MODEL_FAVORITES_STORAGE_KEY));
		} catch {
			snapshot = [];
		}
	}
	return snapshot;
}

export function saveModelFavorites(favorites: ModelFavorite[]) {
	snapshot = parseModelFavorites(JSON.stringify(favorites));
	try {
		window.localStorage.setItem(MODEL_FAVORITES_STORAGE_KEY, JSON.stringify(snapshot));
	} catch {
		// Keep the picker usable when local storage is unavailable.
	}
	listeners.forEach((listener) => listener());
}

export function addModelFavorite(favorite: ModelFavorite) {
	saveModelFavorites([...getModelFavorites(), favorite]);
}

export function removeModelFavorite(favorite: ModelFavorite) {
	const key = modelFavoriteKey(favorite);
	saveModelFavorites(getModelFavorites().filter((entry) => modelFavoriteKey(entry) !== key));
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	const onStorage = (event: StorageEvent) => {
		if (event.key !== MODEL_FAVORITES_STORAGE_KEY && event.key !== null) return;
		snapshot = undefined;
		listener();
	};
	window.addEventListener("storage", onStorage);
	return () => {
		listeners.delete(listener);
		window.removeEventListener("storage", onStorage);
	};
}

export function useModelFavorites() {
	return useSyncExternalStore(subscribe, getModelFavorites, getModelFavorites);
}
