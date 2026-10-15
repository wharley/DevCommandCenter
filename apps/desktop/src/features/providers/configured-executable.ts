import type { ProviderCatalog } from "@dcc/contracts";
import {
	getProviderRuntimeDraft,
	supportsProviderRuntimeBinary,
	type ProviderRuntimeSettings,
} from "./provider-runtime-settings";

type Provider = ProviderCatalog["providers"][number];

/**
 * The executable paths people set in Settings → Advanced. They live in the
 * desktop's settings, so the backend healthcheck never sees them and would
 * call a provider ready while its configured file is missing.
 */
export function configuredExecutables(
	providers: readonly Provider[],
	settings: ProviderRuntimeSettings,
): { providerId: string; path: string }[] {
	return providers.flatMap((provider) => {
		if (!supportsProviderRuntimeBinary(provider.capabilities)) return [];
		const path = getProviderRuntimeDraft(settings, provider.id).binaryPath.trim();
		return path ? [{ providerId: provider.id, path }] : [];
	});
}

/** The same words the adapters use, so the reason reads the same everywhere. */
export function invalidExecutableReason(provider: Provider) {
	return `the configured ${provider.label} executable path is invalid`;
}

/**
 * A provider whose configured executable cannot run needs setup, whatever
 * its healthcheck said: it would fail as soon as a turn starts. Degraded, not
 * Unhealthy: it leaves the picker, but a conversation already on it can still
 * send and get an interrupted turn that says why.
 */
export function withConfiguredExecutableHealth(
	catalog: ProviderCatalog,
	unusableProviderIds: ReadonlySet<string>,
): ProviderCatalog {
	if (unusableProviderIds.size === 0) return catalog;
	return {
		...catalog,
		providers: catalog.providers.map((provider) =>
			unusableProviderIds.has(provider.id)
				? { ...provider, health: { Degraded: { reason: invalidExecutableReason(provider) } } }
				: provider,
		),
	};
}
