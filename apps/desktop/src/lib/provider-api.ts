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
	ProviderAccount,
	ProviderAccountResetInput,
	ProviderAccountResetOutput,
	ProviderAccountSignInOutput,
	ProviderAccountStatusOutput,
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

/** Whether an executable path set in Settings exists and can run. */
export async function checkProviderExecutable(path: string): Promise<boolean> {
	if (!isTauriRuntime()) {
		return true;
	}
	const output = await invoke<{ usable: boolean }>(PROVIDER_METHODS.checkProviderExecutable, {
		input: { path },
	});
	return output.usable;
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

/** Named sign-ins of providers that support accounts. */
export async function listProviderAccounts(): Promise<ProviderAccount[]> {
	if (!isTauriRuntime()) {
		return [];
	}
	const output = await invoke<{ accounts: ProviderAccount[] }>(
		PROVIDER_METHODS.listProviderAccounts,
	);
	return output.accounts.map((account) => ({
		...account,
		providerId: coerceProviderId(account.providerId),
	}));
}

export async function createProviderAccount(input: {
	providerId: string;
	label: string;
	importDir?: string | null;
}): Promise<ProviderAccount> {
	if (!isTauriRuntime()) {
		throw new Error("Provider accounts require the desktop runtime.");
	}
	const output = await invoke<{ account: ProviderAccount }>(
		PROVIDER_METHODS.createProviderAccount,
		{ input: { ...input, importDir: input.importDir ?? null } },
	);
	return output.account;
}

export async function renameProviderAccount(
	accountId: string,
	label: string,
): Promise<ProviderAccount> {
	if (!isTauriRuntime()) {
		throw new Error("Provider accounts require the desktop runtime.");
	}
	const output = await invoke<{ account: ProviderAccount }>(
		PROVIDER_METHODS.renameProviderAccount,
		{ input: { accountId, label } },
	);
	return output.account;
}

export async function removeProviderAccount(accountId: string): Promise<void> {
	if (!isTauriRuntime()) {
		throw new Error("Provider accounts require the desktop runtime.");
	}
	await invoke(PROVIDER_METHODS.removeProviderAccount, {
		input: { accountId },
	});
}

/** Who the account (or, with `null`, the system login) is signed in as. */
export async function getProviderAccountStatus(
	providerId: string,
	accountId: string | null,
): Promise<ProviderAccountStatusOutput> {
	if (!isTauriRuntime()) {
		return { status: null, loginCommand: null };
	}
	return invoke<ProviderAccountStatusOutput>(
		PROVIDER_METHODS.providerAccountStatus,
		{ input: { providerId, accountId } },
	);
}

/**
 * Signs the account (or, with `null`, the system login) in through the
 * browser. Resolves once the person authorized; rejects on cancel or failure.
 */
export async function signInProviderAccount(
	providerId: string,
	accountId: string | null,
): Promise<ProviderAccountSignInOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Provider sign-in requires the desktop runtime.");
	}
	return invoke<ProviderAccountSignInOutput>(PROVIDER_METHODS.signInProviderAccount, {
		input: { providerId, accountId },
	});
}

export async function cancelProviderAccountSignIn(
	providerId: string,
	accountId: string | null,
): Promise<void> {
	if (!isTauriRuntime()) return;
	await invoke(PROVIDER_METHODS.cancelProviderAccountSignIn, {
		input: { providerId, accountId },
	});
}

export async function consumeProviderAccountReset(
	input: ProviderAccountResetInput,
): Promise<ProviderAccountResetOutput> {
	if (!isTauriRuntime()) {
		throw new Error("Provider account resets require the desktop runtime.");
	}
	return invoke<ProviderAccountResetOutput>(
		PROVIDER_METHODS.providerAccountReset,
		{ input },
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
