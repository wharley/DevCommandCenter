import { invoke } from "@tauri-apps/api/core";
import { PROVIDER_METHODS } from "@dcc/contracts";
import type {
	ListProvidersOutput,
	AntigravityStatusOutput,
	InstallAntigravityOutput,
	ConnectAntigravityOutput,
	ProviderAvailabilityInput,
	ProviderAvailabilityOutput,
	ProviderAccountUsageOutput,
	ProviderCatalog,
	ProviderRuntimeConfig,
	SetProviderAvailabilityInput,
} from "@dcc/contracts";
import { FALLBACK_PROVIDER_CATALOG } from "./fallback-provider-catalog";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function coerceProviderId(id: unknown): string {
	if (typeof id === "string") {
		return id;
	}
	if (id !== null && typeof id === "object") {
		const record = id as Record<PropertyKey, unknown>;
		if ("0" in record) {
			return String(record["0"] ?? "");
		}
	}
	return String(id ?? "");
}

export function normalizeProviderCatalog(catalog: ProviderCatalog): ProviderCatalog {
	return {
		providers: catalog.providers.map((provider) => ({
			...provider,
			id: coerceProviderId(provider.id),
		})),
	};
}

/** Loads provider catalog from Tauri; falls back to bundled descriptors when IPC is unavailable or fails. */
export async function listProviders(): Promise<ListProvidersOutput> {
	if (!isTauriRuntime()) {
		return { catalog: FALLBACK_PROVIDER_CATALOG };
	}
	try {
		const raw = await invoke<ListProvidersOutput>(PROVIDER_METHODS.listProviders);
		return { catalog: normalizeProviderCatalog(raw.catalog) };
	} catch (error) {
		console.warn("[dcc] list_providers failed, using bundled catalog", error);
		return { catalog: FALLBACK_PROVIDER_CATALOG };
	}
}

export async function getProviderAvailability(
	input: ProviderAvailabilityInput,
): Promise<ProviderAvailabilityOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Provider availability requires the desktop runtime.");
	}
	return invoke<ProviderAvailabilityOutput>(
		PROVIDER_METHODS.getProviderAvailability,
		{ input },
	);
}

export async function setProviderAvailability(
	input: SetProviderAvailabilityInput,
): Promise<ProviderAvailabilityOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Provider availability can only be changed in the desktop runtime.");
	}
	return invoke<ProviderAvailabilityOutput>(
		PROVIDER_METHODS.setProviderAvailability,
		{ input },
	);
}

export async function getProviderAccountUsage(
	providerId: string,
	providerRuntime: ProviderRuntimeConfig | null,
): Promise<ProviderAccountUsageOutput> {
	if (!isTauriRuntime()) {
		return { usage: null };
	}
	return invoke<ProviderAccountUsageOutput>(
		PROVIDER_METHODS.providerAccountUsage,
		{
			input: {
				providerId,
				providerRuntime,
			},
		},
	);
}

export async function installAntigravity(): Promise<InstallAntigravityOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Antigravity installation requires the desktop runtime.");
	}
	return invoke<InstallAntigravityOutput>(
		PROVIDER_METHODS.installAntigravity,
	);
}

export async function getAntigravityStatus(
	providerRuntime: ProviderRuntimeConfig | null,
): Promise<AntigravityStatusOutput> {
	if (!isTauriRuntime()) {
		return {
			managedRuntimeInstalled: false,
			runtimeVersion: null,
			signedIn: false,
			cachedModelCount: 0,
			lastVerifiedAt: null,
		};
	}
	return invoke<AntigravityStatusOutput>(
		PROVIDER_METHODS.getAntigravityStatus,
		{ input: { providerRuntime } },
	);
}

export async function connectAntigravity(
	providerRuntime: ProviderRuntimeConfig | null,
): Promise<ConnectAntigravityOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Antigravity sign-in requires the desktop runtime.");
	}
	return invoke<ConnectAntigravityOutput>(
		PROVIDER_METHODS.connectAntigravity,
		{ input: { providerRuntime } },
	);
}
