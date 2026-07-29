import type { ProviderCatalog } from "@dcc/contracts";

export type ProviderDescriptor = ProviderCatalog["providers"][number];
export type ProviderModelDescriptor = ProviderDescriptor["models"][number];

export type ProviderChip = {
	label: string;
	variant: "default" | "secondary" | "destructive" | "outline" | "ghost" | "link" | "success" | "warn";
};

function hasVerifiedMcpBridge(provider: ProviderDescriptor): boolean {
	const support = provider.capabilities.mcpSupport;
	return typeof support === "object" && support !== null && "verifiedBridge" in support;
}

function hasRuntimeMcpBridge(provider: ProviderDescriptor): boolean {
	const support = provider.capabilities.mcpSupport;
	return typeof support === "object" && support !== null && "runtimeBridge" in support;
}

export function summarizeProviderHealth(
	health: ProviderDescriptor["health"],
): { label: string; variant: ProviderChip["variant"] } {
	if (health === "Healthy") {
		return { label: "healthy", variant: "success" };
	}

	if (health && typeof health === "object") {
		if ("Degraded" in health) {
			return {
				label: health.Degraded?.reason ?? "degraded",
				variant: "warn",
			};
		}

		if ("Unhealthy" in health) {
			return {
				label: health.Unhealthy?.reason ?? "unhealthy",
				variant: "destructive",
			};
		}
	}

	return { label: "unknown", variant: "outline" };
}

export function getProviderChips(provider: ProviderDescriptor): ProviderChip[] {
	const mcpChip: ProviderChip[] =
		hasVerifiedMcpBridge(provider)
			? [{ label: "mcp verified", variant: "success" }]
			: hasRuntimeMcpBridge(provider)
				? [{ label: "mcp runtime bridge", variant: "warn" }]
				: provider.capabilities.mcpSupport === "nativeConfig"
					? [{ label: "mcp native config", variant: "outline" }]
					: [];

	return [
		{
			label: provider.stable ? "stable" : "experimental",
			variant: provider.stable ? "success" : "outline",
		},
		{
			label: summarizeProviderHealth(provider.health).label,
			variant: summarizeProviderHealth(provider.health).variant,
		},
		...(provider.capabilities.streaming
			? [{ label: "streaming", variant: "outline" as const }]
			: []),
		...(provider.capabilities.tools
			? [{ label: "tools", variant: "outline" as const }]
			: []),
		...mcpChip,
		...(provider.capabilities.resumable
			? [{ label: "resumable", variant: "outline" as const }]
			: []),
		...(provider.capabilities.vision
			? [{ label: "vision", variant: "outline" as const }]
			: []),
	];
}

export function getRecommendedProviderModel(provider: ProviderDescriptor | null) {
	if (!provider) {
		return null;
	}

	return provider.models.find((model) => model.recommended) ?? provider.models[0] ?? null;
}
