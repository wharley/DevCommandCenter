import { createAsyncStoragePersister } from "@tanstack/query-async-storage-persister";
import { focusManager, QueryClient } from "@tanstack/react-query";

const STORAGE_KEY = "dcc-query-cache";

const loggingLocalStorage: Storage = {
	get length() {
		return window.localStorage.length;
	},
	clear: () => window.localStorage.clear(),
	getItem: (key) => window.localStorage.getItem(key),
	key: (index) => window.localStorage.key(index),
	removeItem: (key) => window.localStorage.removeItem(key),
	setItem: (key, value) => {
		try {
			window.localStorage.setItem(key, value);
		} catch (error) {
			const sizeKb = (value.length / 1024).toFixed(1);
			console.error(
				`[dcc] localStorage.setItem failed for "${key}" (${sizeKb} KB)`,
				error,
			);
			throw error;
		}
	},
};

export const dccQueryKeys = {
	shell: ["shell"] as const,
	workspaces: ["workspaces"] as const,
} as const;

export function createDccQueryClient() {
	focusManager.setEventListener((handleFocus) => {
		let unlistenFocus: (() => void) | undefined;
		let unlistenBlur: (() => void) | undefined;

		void import("@tauri-apps/api/event").then(({ listen }) => {
			void listen("tauri://focus", () => handleFocus(true)).then((fn) => {
				unlistenFocus = fn;
			});
			void listen("tauri://blur", () => handleFocus(false)).then((fn) => {
				unlistenBlur = fn;
			});
		});

		return () => {
			unlistenFocus?.();
			unlistenBlur?.();
		};
	});

	return new QueryClient({
		defaultOptions: {
			queries: {
				gcTime: 24 * 60 * 60_000,
				retry: 1,
				refetchOnWindowFocus: true,
				refetchOnReconnect: false,
			},
		},
	});
}

export const dccQueryPersister = createAsyncStoragePersister({
	key: STORAGE_KEY,
	storage: loggingLocalStorage,
});
