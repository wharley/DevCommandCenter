import type { ProviderCatalog } from "@dcc/contracts";

type Provider = ProviderCatalog["providers"][number];

/** The one line under a provider's name in Settings → Providers. */
export type ProviderRailStatus = {
	tone: "ready" | "attention" | "off";
	/** i18n key under `settings.model`. */
	key: "railOff" | "railAttention" | "railAccounts" | "railReady";
	count?: number;
};

/**
 * What a person most needs to know at a glance: off, needing setup, or ready
 * — and, for providers with accounts, how many sign-ins it has (the system
 * login counts as one). Unhealthy and Degraded read the same: either way the
 * provider is left out of the picker until someone sets it up.
 */
export function providerRailStatus(
	provider: Provider,
	namedAccounts: number,
): ProviderRailStatus {
	if (provider.enabled === false) return { tone: "off", key: "railOff" };
	const health = provider.health;
	if (health && typeof health === "object") {
		if ("Unhealthy" in health || "Degraded" in health) {
			return { tone: "attention", key: "railAttention" };
		}
	}
	if (provider.capabilities.supportsAccounts && namedAccounts > 0) {
		return { tone: "ready", key: "railAccounts", count: namedAccounts + 1 };
	}
	return { tone: "ready", key: "railReady" };
}
