import type { ForgeCliProvider } from "@dcc/contracts";
import { normalizeForgeHost } from "./forge-cli";

const FORGE_ACCOUNT_PREFERENCES_KEY = "dcc.forge-account-preferences.v1";

type ForgeAccountPreferences = Record<string, string>;

function preferenceKey(provider: ForgeCliProvider, host?: string | null): string {
	return `${provider}:${normalizeForgeHost(provider, host)}`;
}

function readAll(): ForgeAccountPreferences {
	if (typeof window === "undefined") {
		return {};
	}

	try {
		const raw = window.localStorage.getItem(FORGE_ACCOUNT_PREFERENCES_KEY);
		if (!raw) {
			return {};
		}
		const parsed = JSON.parse(raw);
		if (!parsed || typeof parsed !== "object") {
			return {};
		}
		return Object.fromEntries(
			Object.entries(parsed).filter(
				([key, value]) => key.length > 0 && typeof value === "string" && value.trim().length > 0,
			),
		) as ForgeAccountPreferences;
	} catch {
		return {};
	}
}

function writeAll(next: ForgeAccountPreferences) {
	if (typeof window === "undefined") {
		return;
	}

	try {
		window.localStorage.setItem(FORGE_ACCOUNT_PREFERENCES_KEY, JSON.stringify(next));
	} catch {
		/* localStorage unavailable */
	}
}

export function readSelectedForgeLogin(
	provider: ForgeCliProvider,
	host?: string | null,
): string | null {
	const value = readAll()[preferenceKey(provider, host)]?.trim();
	return value && value.length > 0 ? value : null;
}

export function writeSelectedForgeLogin(
	provider: ForgeCliProvider,
	host: string | null | undefined,
	login: string | null,
) {
	const key = preferenceKey(provider, host);
	const current = readAll();
	const next = { ...current };
	const normalizedLogin = login?.trim() ?? "";
	if (normalizedLogin.length === 0) {
		delete next[key];
	} else {
		next[key] = normalizedLogin;
	}
	writeAll(next);
}
