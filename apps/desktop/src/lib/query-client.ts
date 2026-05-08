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
	repositories: ["repositories"] as const,
	workspaces: ["workspaces"] as const,
	sessions: ["sessions"] as const,
	sessionThreads: (sessionId: string) => ["sessionThreads", sessionId] as const,
	workspaceSessions: (workspaceId: string) =>
		["workspaceSessions", workspaceId] as const,
} as const;

function shouldRefreshInspectorGitQueries(payload: unknown) {
	if (!payload || typeof payload !== "object") {
		return false;
	}

	const event = payload as Record<string, unknown>;
	return Boolean(
		event.workspacePrepared ||
			event.workspaceReady ||
			event.sessionCompleted ||
			event.sessionTurnCompleted ||
			event.sessionTurnAborted ||
			event.sessionTurnToolCallCompleted ||
			event.sessionTurnToolCallFailed,
	);
}

export function createDccQueryClient() {
	const queryClient = new QueryClient({
		defaultOptions: {
			queries: {
				gcTime: 24 * 60 * 60_000,
				retry: 1,
				refetchOnWindowFocus: true,
				refetchOnReconnect: false,
			},
		},
	});

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

	void import("@tauri-apps/api/event").then(({ listen }) => {
		void listen("dcc:core-event", (event) => {
			void queryClient.invalidateQueries({ queryKey: dccQueryKeys.repositories });
			void queryClient.invalidateQueries({ queryKey: dccQueryKeys.workspaces });
			void queryClient.invalidateQueries({ queryKey: dccQueryKeys.sessions });
			void queryClient.invalidateQueries({ queryKey: ["sessionThreads"] });
			void queryClient.invalidateQueries({ queryKey: ["workspaceSessions"] });
			if (shouldRefreshInspectorGitQueries(event.payload)) {
				void queryClient.invalidateQueries({
					predicate: (query) => {
						const queryKey = query.queryKey[0];
						return (
							queryKey === "workspaceGitStatus" ||
							queryKey === "workspacePrStatus" ||
							queryKey === "workspaceGitBranchDiff"
						);
					},
				});
			}
		});
	});

	return queryClient;
}

export const dccQueryPersister = createAsyncStoragePersister({
	key: STORAGE_KEY,
	storage: loggingLocalStorage,
});
