import type { ProviderCatalog } from "@dcc/contracts";
import {
	isProviderEnabled,
	isProviderReady,
} from "@/features/providers/provider-selection.logic";
import {
	modelFavoriteKey,
	resolveModelFavorite,
	type ModelFavorite,
} from "./model-favorites";

type PickerProvider = ProviderCatalog["providers"][number];

export const FAVORITES_TAB = "favorites";
export const MAX_SHORTCUT_ROWS = 9;

export type ModelPickerRow = {
	key: string;
	providerId: string;
	modelId: string;
	label: string;
	description: string;
	/** Set on favorite rows: picking applies this effort too (null = managed). */
	favorite: ModelFavorite | null;
	/** Matches from providers other than the open tab while searching. */
	fromOtherProvider: boolean;
};

/**
 * Only providers that can start reach the picker: switched on and healthy.
 * The catalog still carries the rest (history labels and Settings need them),
 * so the hiding is a picker concern. The provider already selected stays, so a
 * conversation on a provider that broke still shows why.
 */
export function pickerProviders(
	providers: ProviderCatalog["providers"],
	keepProviderId: string | null = null,
) {
	return providers.filter(
		(provider) =>
			isProviderReady(provider) ||
			(provider.id === keepProviderId && isProviderEnabled(provider)),
	);
}

/** Switched-on providers the picker leaves out because they need setup. */
export function providersNeedingSetup(
	providers: ProviderCatalog["providers"],
	keepProviderId: string | null = null,
) {
	return providers.filter(
		(provider) =>
			isProviderEnabled(provider) &&
			!isProviderReady(provider) &&
			provider.id !== keepProviderId,
	);
}

/** Cursor exposes every routed variant; only its plain models belong in a list. */
export function pickerModels(provider: PickerProvider) {
	if (provider.id !== "cursor") return provider.models;
	return provider.models.filter(
		(model) =>
			model.id.trim().toLowerCase() === "auto" || !model.id.includes(" - "),
	);
}

function matches(query: string, haystack: string) {
	const tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
	if (tokens.length === 0) return true;
	const target = haystack.toLowerCase();
	return tokens.every((token) => target.includes(token));
}

function modelRow(
	provider: PickerProvider,
	model: PickerProvider["models"][number],
	fromOtherProvider: boolean,
): ModelPickerRow {
	return {
		key: `${provider.id}:${model.id}`,
		providerId: provider.id,
		modelId: model.id,
		label: model.label,
		description: model.description,
		favorite: null,
		fromOtherProvider,
	};
}

export function buildModelPickerRows(input: {
	providers: ProviderCatalog["providers"];
	favorites: ModelFavorite[];
	activeTab: string;
	query: string;
	selectedProviderId?: string | null;
}): ModelPickerRow[] {
	const providers = pickerProviders(input.providers, input.selectedProviderId ?? null);
	const query = input.query.trim();

	if (input.activeTab === FAVORITES_TAB) {
		const favoriteRows = input.favorites.flatMap((favorite) => {
			const { provider, model, available } = resolveModelFavorite(
				favorite,
				providers,
			);
			if (!available || !provider || !model) return [];
			if (!matches(query, `${provider.label} ${model.label} ${model.id}`)) return [];
			return [{
				...modelRow(provider, model, false),
				key: modelFavoriteKey(favorite),
				favorite,
			}];
		});
		if (!query) return favoriteRows;
		// Searching from favorites still reaches every provider's models.
		const starred = new Set(
			favoriteRows.map((row) => `${row.providerId}:${row.modelId}`),
		);
		const others = searchProviders(providers, null, query).filter(
			(row) => !starred.has(row.key),
		);
		return [...favoriteRows, ...others];
	}

	const active = providers.find((provider) => provider.id === input.activeTab);
	const own = active
		? pickerModels(active)
				.filter((model) => matches(query, `${model.label} ${model.id}`))
				.map((model) => modelRow(active, model, false))
		: [];
	if (!query) return own;

	// A search that misses the open tab still finds the model elsewhere.
	return [...own, ...searchProviders(providers, active?.id ?? null, query)];
}

function searchProviders(
	providers: ProviderCatalog["providers"],
	skipProviderId: string | null,
	query: string,
) {
	return providers
		.filter((provider) => provider.id !== skipProviderId)
		.flatMap((provider) =>
			pickerModels(provider)
				.filter((model) =>
					matches(query, `${provider.label} ${model.label} ${model.id}`),
				)
				.map((model) => modelRow(provider, model, true)),
		);
}

/** The tab the picker opens on: favorites when any can be used, else the current provider. */
export function initialModelPickerTab(input: {
	providers: ProviderCatalog["providers"];
	favorites: ModelFavorite[];
	selectedProviderId: string | null;
}) {
	const providers = pickerProviders(input.providers, input.selectedProviderId);
	const hasUsableFavorite = input.favorites.some(
		(favorite) => resolveModelFavorite(favorite, providers).available,
	);
	if (hasUsableFavorite) return FAVORITES_TAB;
	return (
		providers.find((provider) => provider.id === input.selectedProviderId)?.id ??
		providers[0]?.id ??
		FAVORITES_TAB
	);
}

/** `⌘1`…`⌘9` (Ctrl off macOS) → row index, or null for any other key. */
export function shortcutRowIndex(
	event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
	isMac: boolean,
) {
	const modifier = isMac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
	if (!modifier || event.altKey || event.shiftKey) return null;
	if (!/^[1-9]$/.test(event.key)) return null;
	return Number(event.key) - 1;
}

/** Effort stops for the slider; ultrathink rides on top of the model's own ladder. */
export function effortSliderSteps(levels: readonly string[]) {
	return levels.length > 0 ? [...levels, "ultrathink"] : [];
}

export function isModelFavorited(
	favorites: ModelFavorite[],
	providerId: string,
	modelId: string,
) {
	return favorites.some(
		(favorite) => favorite.providerId === providerId && favorite.modelId === modelId,
	);
}
