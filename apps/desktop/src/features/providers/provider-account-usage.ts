import { useQuery } from "@tanstack/react-query";
import type {
	Capabilities,
	ProviderAccountUsage,
	ProviderRuntimeConfig,
	ProviderUsageWindow,
} from "@dcc/contracts";
import { getProviderAccountUsage } from "@/lib/provider-api";

export type ProviderUsageSeverity = "warning" | "critical" | null;

export type ProviderUsageCandidate = {
	id: string;
	capabilities: Pick<Capabilities, "supportsAccountUsage">;
};

/**
 * Account usage is a backend-declared capability projected through the
 * provider catalog. The renderer never infers it from the provider id.
 */
export function supportsProviderAccountUsage(
	provider: ProviderUsageCandidate | null | undefined,
): boolean {
	return provider?.capabilities.supportsAccountUsage === true;
}

export function mostConstrainedUsageWindow(
	usage: ProviderAccountUsage | null | undefined,
): ProviderUsageWindow | null {
	if (!usage || usage.state !== "available" || usage.windows.length === 0) {
		return null;
	}
	return usage.windows.reduce((current, candidate) => {
		if (candidate.isExhausted !== current.isExhausted) {
			return candidate.isExhausted ? candidate : current;
		}
		return candidate.remainingPercent < current.remainingPercent
			? candidate
			: current;
	});
}

export function providerUsageSeverity(
	window: ProviderUsageWindow | null,
): ProviderUsageSeverity {
	if (!window) return null;
	if (window.isExhausted || window.remainingPercent <= 5) return "critical";
	if (window.remainingPercent <= 20) return "warning";
	return null;
}

export function useProviderAccountUsage(
	provider: ProviderUsageCandidate | null | undefined,
	providerRuntime: ProviderRuntimeConfig | null,
) {
	const providerId = provider?.id ?? null;
	const supported = supportsProviderAccountUsage(provider);
	return useQuery({
		queryKey: ["provider-account-usage", providerId, supported, providerRuntime],
		queryFn: async () => {
			if (!providerId || !supported) {
				return null;
			}
			return (await getProviderAccountUsage(providerId, providerRuntime)).usage;
		},
		enabled: false,
		staleTime: 30_000,
		refetchOnWindowFocus: false,
		retry: false,
	});
}
