import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import { providerRailStatus } from "./providers-settings.logic";

const claude = FALLBACK_PROVIDER_CATALOG.providers.find((provider) => provider.id === "claude_code")!;
const cursor = FALLBACK_PROVIDER_CATALOG.providers.find((provider) => provider.id === "cursor")!;

describe("provider rail status", () => {
	it("says off first, then needing setup however the provider broke", () => {
		expect(providerRailStatus({ ...claude, enabled: false }, 2)).toEqual({
			tone: "off",
			key: "railOff",
		});
		expect(
			providerRailStatus({ ...claude, health: { Unhealthy: { reason: "not signed in" } } }, 0).key,
		).toBe("railAttention");
		expect(
			providerRailStatus({ ...claude, health: { Degraded: { reason: "slow" } } }, 0).key,
		).toBe("railAttention");
	});

	it("counts the system login with the named accounts", () => {
		expect(providerRailStatus({ ...claude, health: "Healthy" }, 2)).toEqual({
			tone: "ready",
			key: "railAccounts",
			count: 3,
		});
		expect(providerRailStatus({ ...claude, health: "Healthy" }, 0).key).toBe("railReady");
		expect(providerRailStatus({ ...cursor, health: "Healthy" }, 4).key).toBe("railReady");
	});
});
