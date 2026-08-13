import type { ProviderCatalog } from "@dcc/contracts";

export function resolveNativeSubagentModelName(
	model: string | null | undefined,
	providers: ProviderCatalog["providers"] = [],
) {
	const normalized = model?.trim();
	if (!normalized) return null;

	const label =
		providers
			.flatMap((provider) => provider.models)
			.find((candidate) => candidate.id === normalized)?.label ?? normalized;
	return label;
}

export function presentableNativeSubagentName(name: string | null | undefined) {
	const normalized = name?.trim();
	if (!normalized || normalized.startsWith("/") || normalized.startsWith("root/")) {
		return null;
	}
	return normalized;
}

export function resolveNativeSubagentPresentation(
	annotation: {
		model?: string | null;
		requestedModel?: string | null;
		name?: string | null;
		role?: string | null;
	},
	providers: ProviderCatalog["providers"] = [],
) {
	const modelName = resolveNativeSubagentModelName(annotation.model, providers);
	const requestedModelName = resolveNativeSubagentModelName(
		annotation.requestedModel,
		providers,
	);
	const agentName = presentableNativeSubagentName(annotation.name);
	return {
		modelName,
		requestedModelName,
		agentName,
		identity: modelName ?? requestedModelName ?? agentName ?? annotation.role ?? null,
	};
}
