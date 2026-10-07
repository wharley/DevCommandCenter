import type { ProviderCatalog } from "@dcc/contracts";
import { isProviderEnabled } from "@/features/providers/provider-selection.logic";
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
 * Providers switched off in Settings never reach the picker. The catalog still
 * carries them (history labels and Settings need them), so the hiding is a
 * picker concern, like Synara and T3 Code do it.
 */
export function pickerProviders(providers: ProviderCatalog["providers"]) {
	return providers.filter(isProviderEnabled);
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
}): ModelPickerRow[] {
	const providers = pickerProviders(input.providers);
	const query = input.query.trim();

	if (input.activeTab === FAVORITES_TAB) {
		return input.favorites.flatMap((favorite) => {
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
	}

	const active = providers.find((provider) => provider.id === input.activeTab);
	const own = active
		? pickerModels(active)
				.filter((model) => matches(query, `${model.label} ${model.id}`))
				.map((model) => modelRow(active, model, false))
		: [];
	if (!query) return own;

	// A search that misses the open tab still finds the model elsewhere.
	const others = providers
		.filter((provider) => provider.id !== active?.id)
		.flatMap((provider) =>
			pickerModels(provider)
				.filter((model) =>
					matches(query, `${provider.label} ${model.label} ${model.id}`),
				)
				.map((model) => modelRow(provider, model, true)),
		);
	return [...own, ...others];
}

/** The tab the picker opens on: favorites when any can be used, else the current provider. */
export function initialModelPickerTab(input: {
	providers: ProviderCatalog["providers"];
	favorites: ModelFavorite[];
	selectedProviderId: string | null;
}) {
	const providers = pickerProviders(input.providers);
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
