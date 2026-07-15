import { useQuery } from "@tanstack/react-query";
import type {
	ProviderAccountUsage,
	ProviderRuntimeConfig,
	ProviderUsageWindow,
} from "@dcc/contracts";
import { getProviderAccountUsage } from "@/lib/provider-api";

const SUPPORTED_PROVIDER_IDS = new Set(["codex", "claude_code"]);

export type ProviderUsageSeverity = "warning" | "critical" | null;

export function supportsProviderAccountUsage(providerId: string | null): boolean {
	return Boolean(providerId && SUPPORTED_PROVIDER_IDS.has(providerId));
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
	providerId: string | null,
	providerRuntime: ProviderRuntimeConfig | null,
) {
	return useQuery({
		queryKey: ["provider-account-usage", providerId, providerRuntime],
		queryFn: async () => {
			if (!providerId || !supportsProviderAccountUsage(providerId)) {
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
