import type { ProviderCatalog } from "@dcc/contracts";

type Provider = ProviderCatalog["providers"][number];

/** The one line under a provider's name in Settings → Providers. */
export type ProviderRailStatus = {
	tone: "ready" | "attention" | "unavailable" | "off";
	/** i18n key under `settings.model`. */
	key: "railOff" | "railUnavailable" | "railAttention" | "railAccounts" | "railReady";
	count?: number;
};

/**
 * What a person most needs to know at a glance: off, broken, needing them,
 * or ready — and, for providers with accounts, how many sign-ins it has
 * (the system login counts as one).
 */
export function providerRailStatus(
	provider: Provider,
	namedAccounts: number,
): ProviderRailStatus {
	if (provider.enabled === false) return { tone: "off", key: "railOff" };
	const health = provider.health;
	if (health && typeof health === "object") {
		if ("Unhealthy" in health) return { tone: "unavailable", key: "railUnavailable" };
		if ("Degraded" in health) return { tone: "attention", key: "railAttention" };
	}
	if (provider.capabilities.supportsAccounts && namedAccounts > 0) {
		return { tone: "ready", key: "railAccounts", count: namedAccounts + 1 };
	}
	return { tone: "ready", key: "railReady" };
}
