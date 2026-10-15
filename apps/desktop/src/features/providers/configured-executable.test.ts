import { describe, expect, it } from "vitest";
import { FALLBACK_PROVIDER_CATALOG } from "@/lib/fallback-provider-catalog";
import {
	configuredExecutables,
	withConfiguredExecutableHealth,
} from "./configured-executable";
import { getProviderSetupReason, isProviderReady } from "./provider-selection.logic";
import { providerSetupMessage } from "./provider-setup-message";

const draft = (binaryPath: string) => ({
	binaryPath,
	homePath: "",
	shadowHomePath: "",
	maxConcurrentSubagents: "",
});

describe("configured executables", () => {
	it("lists only the paths set for providers that take one", () => {
		const providers = FALLBACK_PROVIDER_CATALOG.providers;
		const antigravity = providers.find((provider) => provider.id === "antigravity")!;
		const listed = configuredExecutables(providers, {
			antigravity: draft(" /tmp/dcc-quebra/agy_acp_server.par "),
			claude_code: draft(""),
		} as never);
		expect(listed).toEqual([
			{ providerId: "antigravity", path: "/tmp/dcc-quebra/agy_acp_server.par" },
		]);
		expect(antigravity.capabilities.supportsRuntimeBinary).toBe(true);
	});

	it("marks a provider whose configured file cannot run as needing setup", () => {
		const catalog = {
			...FALLBACK_PROVIDER_CATALOG,
			providers: FALLBACK_PROVIDER_CATALOG.providers.map((provider) =>
				provider.id === "codex" ? { ...provider, health: "Healthy" as const } : provider,
			),
		};
		const codex = withConfiguredExecutableHealth(catalog, new Set(["codex"])).providers.find(
			(provider) => provider.id === "codex",
		)!;
		expect(isProviderReady(codex)).toBe(false);
		const reason = getProviderSetupReason(codex)!;
		expect(providerSetupMessage(reason)?.issue).toBe("invalidExecutable");
		expect(withConfiguredExecutableHealth(catalog, new Set())).toBe(catalog);
	});

	it("recognizes the adapters' own invalid-path error", () => {
		expect(providerSetupMessage("provider error: the configured Antigravity executable path is invalid")?.issue)
			.toBe("invalidExecutable");
	});
});
