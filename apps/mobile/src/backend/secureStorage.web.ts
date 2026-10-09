/**
 * The web build has no Keychain: localStorage, readable by any script on the
 * origin. Fine for the demo and local testing; the native app is the product.
 */
export async function getItem(key: string): Promise<string | null> {
	return globalThis.localStorage?.getItem(key) ?? null;
}

export async function setItem(key: string, value: string): Promise<void> {
	globalThis.localStorage?.setItem(key, value);
}

export async function deleteItem(key: string): Promise<void> {
	globalThis.localStorage?.removeItem(key);
}
