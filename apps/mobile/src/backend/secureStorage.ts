/** Keychain (iOS) / Keystore (Android) storage for the device key and pairing. */
import * as SecureStore from "expo-secure-store";

const options: SecureStore.SecureStoreOptions = {
	// Reconnecting in the background after the first unlock is fine; the key
	// stays on this device (not in iCloud Keychain backups).
	keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
};

export function getItem(key: string): Promise<string | null> {
	return SecureStore.getItemAsync(key, options);
}

export function setItem(key: string, value: string): Promise<void> {
	return SecureStore.setItemAsync(key, value, options);
}

export function deleteItem(key: string): Promise<void> {
	return SecureStore.deleteItemAsync(key, options);
}
