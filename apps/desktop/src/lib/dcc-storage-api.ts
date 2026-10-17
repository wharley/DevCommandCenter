import { invoke } from "@tauri-apps/api/core";

export type DccStorageUsage = {
	/** The SQLite file plus its WAL. */
	databaseBytes: number;
	/** Free pages inside the database file. */
	reclaimableBytes: number;
	/** The free pages only leave the file when DCC opens again. */
	reclaimsOnRestart: boolean;
	browserCacheBytes: number;
	/** The app's own webview storage (localStorage and its WAL). */
	appWebviewBytes: number;
};

export function readDccStorageUsage() {
	return invoke<DccStorageUsage>("dcc_storage_usage");
}

/** Merges settled history, returns free pages and drops browser caches. */
export function compactDccStorage() {
	return invoke<DccStorageUsage>("dcc_storage_compact");
}

export function dccStorageBytes(usage: DccStorageUsage): number {
	return usage.databaseBytes + usage.browserCacheBytes + usage.appWebviewBytes;
}
