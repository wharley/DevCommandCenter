/**
 * The access level picked on this phone, per project — the desktop keeps one
 * per composer in localStorage (`draftStorage.ts`). A small preference, kept
 * with the other ones in the Keychain/Keystore store.
 */
import { isAccessPolicy, type AccessPolicy } from "@dcc/mobile-core";
import { getItem, setItem } from "./secureStorage";

const memory = new Map<string, AccessPolicy | null>();

function storageKey(projectKey: string): string {
	// Keychain keys only take letters, digits, ".", "-" and "_".
	return `dcc.access.${projectKey.replace(/[^\w.-]/g, "_")}`;
}

export async function loadProjectAccess(projectKey: string): Promise<AccessPolicy | null> {
	if (memory.has(projectKey)) return memory.get(projectKey) ?? null;
	const stored = await getItem(storageKey(projectKey)).catch(() => null);
	const policy = isAccessPolicy(stored) ? stored : null;
	memory.set(projectKey, policy);
	return policy;
}

export function saveProjectAccess(projectKey: string, policy: AccessPolicy): void {
	memory.set(projectKey, policy);
	void setItem(storageKey(projectKey), policy).catch(() => {});
}
