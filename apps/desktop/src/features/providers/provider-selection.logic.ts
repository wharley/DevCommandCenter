import type { ProviderCatalog } from "@dcc/contracts";

export const SELECTED_PROVIDER_STORAGE_KEY = "dcc.selectedProviderId";
export const SELECTED_MODEL_STORAGE_KEY = "dcc.selectedModelId";

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
		const match = provider.models.find((model) => model.id === storedModelId);
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
