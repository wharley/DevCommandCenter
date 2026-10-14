import { createContext, useContext, useSyncExternalStore } from "react";
import type { ProviderAccount, ProviderRuntimeConfig } from "@dcc/contracts";

export const PROVIDER_ACCOUNTS_QUERY_KEY = ["providers", "accounts"] as const;

export const PROVIDER_ACCOUNT_SELECTION_STORAGE_KEY = "dcc.providerAccountSelection";

/** Account picked for new conversations, per provider. Absent = system login. */
export type ProviderAccountSelection = Readonly<Record<string, string>>;

/** The account a running conversation is pinned to (`null` = system login). */
export type ConversationAccount = {
	providerId: string;
	accountId: string | null;
};

/**
 * What the composer's account picker needs from the app: the stored accounts
 * and the account the open conversation is pinned to. Without a provider the
 * picker shows no account choice.
 */
export const ProviderAccountsContext = createContext<{
	accounts: readonly ProviderAccount[];
	conversation: ConversationAccount | null;
}>({ accounts: [], conversation: null });

/**
 * The account the picker shows as chosen for a provider: the conversation's
 * own account while it stays on that provider, else the one picked for new
 * conversations.
 */
export function shownAccountId(
	providerId: string,
	conversation: ConversationAccount | null,
	selection: ProviderAccountSelection,
	accounts: readonly ProviderAccount[],
): { accountId: string | null; locked: boolean } {
	const locked = conversation?.providerId === providerId;
	return {
		accountId: resolveTurnAccountId({ providerId, conversation, selection, accounts }),
		locked,
	};
}

/** Name of the account a provider runs as, when it is not the system login. */
export function useShownAccountLabel(providerId: string | null | undefined): string | null {
	const { accounts, conversation } = useContext(ProviderAccountsContext);
	const selection = useProviderAccountSelection();
	if (!providerId) return null;
	const { accountId } = shownAccountId(providerId, conversation, selection, accounts);
	return accounts.find((account) => account.id === accountId)?.label ?? null;
}

/** One provider's accounts, oldest first (the order they were added). */
export function accountsOfProvider(
	accounts: readonly ProviderAccount[],
	providerId: string,
): ProviderAccount[] {
	return accounts
		.filter((account) => account.providerId === providerId)
		.sort((left, right) => left.createdAt.localeCompare(right.createdAt));
}

export function parseProviderAccountSelection(raw: string | null): ProviderAccountSelection {
	if (!raw) return {};
	try {
		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
		return Object.fromEntries(
			Object.entries(parsed as Record<string, unknown>).filter(
				(entry): entry is [string, string] =>
					typeof entry[1] === "string" && entry[1].length > 0,
			),
		);
	} catch {
		return {};
	}
}

/**
 * The account a turn runs as: the conversation's own account while it stays on
 * its provider, else the account picked for new conversations. A picked
 * account that was removed falls back to the system login.
 */
export function resolveTurnAccountId({
	providerId,
	conversation,
	selection,
	accounts,
}: {
	providerId: string | null;
	conversation: ConversationAccount | null;
	selection: ProviderAccountSelection;
	accounts: readonly ProviderAccount[];
}): string | null {
	if (!providerId) return null;
	if (conversation?.providerId === providerId) return conversation.accountId;
	const picked = selection[providerId];
	return picked && accounts.some((account) => account.id === picked && account.providerId === providerId)
		? picked
		: null;
}

/** Adds the account to the provider runtime config the backend receives. */
export function withProviderAccount(
	runtime: ProviderRuntimeConfig | null,
	accountId: string | null,
): ProviderRuntimeConfig | null {
	if (!accountId) return runtime;
	return { ...(runtime ?? {}), accountId };
}

let snapshot: ProviderAccountSelection | undefined;
const listeners = new Set<() => void>();

export function getProviderAccountSelection(): ProviderAccountSelection {
	if (!snapshot) {
		try {
			snapshot = parseProviderAccountSelection(
				window.localStorage.getItem(PROVIDER_ACCOUNT_SELECTION_STORAGE_KEY),
			);
		} catch {
			snapshot = {};
		}
	}
	return snapshot;
}

export function selectProviderAccount(providerId: string, accountId: string | null) {
	const next = { ...getProviderAccountSelection() };
	if (accountId) next[providerId] = accountId;
	else delete next[providerId];
	snapshot = next;
	try {
		window.localStorage.setItem(PROVIDER_ACCOUNT_SELECTION_STORAGE_KEY, JSON.stringify(next));
	} catch {
		// The selection still holds for this window without local storage.
	}
	listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
	listeners.add(listener);
	const onStorage = (event: StorageEvent) => {
		if (event.key !== PROVIDER_ACCOUNT_SELECTION_STORAGE_KEY && event.key !== null) return;
		snapshot = undefined;
		listener();
	};
	window.addEventListener("storage", onStorage);
	return () => {
		listeners.delete(listener);
		window.removeEventListener("storage", onStorage);
	};
}

export function useProviderAccountSelection() {
	return useSyncExternalStore(
		subscribe,
		getProviderAccountSelection,
		getProviderAccountSelection,
	);
}
