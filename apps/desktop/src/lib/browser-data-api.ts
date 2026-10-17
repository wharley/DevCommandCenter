import { invoke } from "@tauri-apps/api/core";

export type BrowserStorageUsage = {
	/** False where the built-in browser keeps nothing on disk. */
	supported: boolean;
	cacheBytes: number;
	totalBytes: number;
};

export function readBrowserStorageUsage() {
	return invoke<BrowserStorageUsage>("browser_storage_usage");
}

/** `everything` also signs the browser out of every site. */
export function clearBrowserData(everything: boolean) {
	return invoke<BrowserStorageUsage>("browser_clear_data", { everything });
}
