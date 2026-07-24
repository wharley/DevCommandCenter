import { createAsyncStoragePersister } from "@tanstack/query-async-storage-persister";
import {
	defaultShouldDehydrateQuery,
	focusManager,
	QueryClient,
	type Query,
} from "@tanstack/react-query";
import {
	removeOldestQuery,
	type PersistedClient,
} from "@tanstack/react-query-persist-client";
import { REMOTE_CORE_EVENT_NAME } from "./session-api";

export const DCC_QUERY_CACHE_STORAGE_KEY = "dcc-query-cache";
export const DCC_QUERY_CACHE_BUSTER = "dcc-query-cache-v2";

/**
 * WebKit gives localStorage a small, shared per-origin quota. Keep the query
 * snapshot well below it so drafts and preferences always retain headroom.
 */
export const DCC_QUERY_CACHE_MAX_CHARS = 1_000_000;

const PERSISTED_QUERY_ROOT_PRIORITY = new Map<string, number>([
	["shell", 0],
	["repositories", 1],
	["workspaces", 2],
	["providers", 3],
	["sessions", 4],
	["workspaceSessions", 5],
	["sessionThreads", 6],
]);

function persistedQueryRoot(queryKey: readonly unknown[]): string | null {
	const root = queryKey[0];
	return typeof root === "string" ? root : null;
}

export function shouldPersistDccQuery(query: Query): boolean {
	const root = persistedQueryRoot(query.queryKey);
	return (
		root !== null &&
		PERSISTED_QUERY_ROOT_PRIORITY.has(root) &&
		defaultShouldDehydrateQuery(query)
	);
}

type DehydratedQuery = PersistedClient["clientState"]["queries"][number];

function persistedQueryPriority(query: DehydratedQuery): number {
	const root = persistedQueryRoot(query.queryKey);
	return root === null
		? Number.MAX_SAFE_INTEGER
		: (PERSISTED_QUERY_ROOT_PRIORITY.get(root) ?? Number.MAX_SAFE_INTEGER);
}

/**
 * Retains the most useful and most recent query snapshots within a hard size
 * budget. The live QueryClient is untouched; this only shapes the restart
 * snapshot written to localStorage.
 */
export function compactDccPersistedClient(
	persistedClient: PersistedClient,
	maxChars = DCC_QUERY_CACHE_MAX_CHARS,
): PersistedClient {
	const compacted: PersistedClient = {
		...persistedClient,
		clientState: {
			mutations: [],
			queries: [],
		},
	};
	const baseSize = JSON.stringify(compacted).length;
	let usedChars = baseSize;

	const candidates = persistedClient.clientState.queries
		.filter((query) => {
			const root = persistedQueryRoot(query.queryKey);
			return root !== null && PERSISTED_QUERY_ROOT_PRIORITY.has(root);
		})
		.map((query) => ({ query, serialized: JSON.stringify(query) }))
		.sort((left, right) => {
			const priorityDelta =
				persistedQueryPriority(left.query) - persistedQueryPriority(right.query);
			if (priorityDelta !== 0) {
				return priorityDelta;
			}
			return right.query.state.dataUpdatedAt - left.query.state.dataUpdatedAt;
		});

	for (const candidate of candidates) {
		const separatorSize = compacted.clientState.queries.length === 0 ? 0 : 1;
		const candidateSize = candidate.serialized.length + separatorSize;
		if (usedChars + candidateSize > maxChars) {
			continue;
		}
		compacted.clientState.queries.push(candidate.query);
		usedChars += candidateSize;
	}

	return compacted;
}

export function serializeDccQueryCache(persistedClient: PersistedClient): string {
	return JSON.stringify(compactDccPersistedClient(persistedClient));
}

function isQuotaExceededError(error: unknown): boolean {
	if (!error || typeof error !== "object") {
		return false;
	}
	const candidate = error as { name?: unknown; code?: unknown };
	return (
		candidate.name === "QuotaExceededError" ||
		candidate.name === "NS_ERROR_DOM_QUOTA_REACHED" ||
		candidate.code === 22 ||
		candidate.code === 1014
	);
}

/** Removes only an obsolete/corrupt query cache; all other local data stays. */
export function prepareDccQueryCacheStorage(storage: Storage): void {
	try {
		const raw = storage.getItem(DCC_QUERY_CACHE_STORAGE_KEY);
		if (!raw) {
			return;
		}
		const parsed = JSON.parse(raw) as { buster?: unknown };
		if (
			parsed.buster !== DCC_QUERY_CACHE_BUSTER ||
			raw.length > DCC_QUERY_CACHE_MAX_CHARS
		) {
			storage.removeItem(DCC_QUERY_CACHE_STORAGE_KEY);
		}
	} catch {
		try {
			storage.removeItem(DCC_QUERY_CACHE_STORAGE_KEY);
		} catch {
			/* localStorage unavailable; continue without persisted query cache */
		}
	}
}

export function createDccQueryCacheStorage(storage: Storage): Storage {
	return {
		get length() {
			return storage.length;
		},
		clear: () => storage.clear(),
		getItem: (key) => storage.getItem(key),
		key: (index) => storage.key(index),
		removeItem: (key) => storage.removeItem(key),
		setItem: (key, value) => {
			try {
				storage.setItem(key, value);
			} catch (error) {
				const sizeKb = (value.length / 1024).toFixed(1);
				console.error(
					`[dcc] localStorage.setItem failed for "${key}" (${sizeKb} KB)`,
					error,
				);

				if (key === DCC_QUERY_CACHE_STORAGE_KEY && isQuotaExceededError(error)) {
					storage.removeItem(DCC_QUERY_CACHE_STORAGE_KEY);
					storage.setItem(key, value);
					return;
				}

				throw error;
			}
		},
	};
}

function getBrowserStorage(): Storage | undefined {
	if (typeof window === "undefined") {
		return undefined;
	}
	try {
		return window.localStorage;
	} catch {
		return undefined;
	}
}

const browserStorage = getBrowserStorage();
if (browserStorage) {
	prepareDccQueryCacheStorage(browserStorage);
}

export const dccQueryKeys = {
	shell: ["shell"] as const,
	repositories: ["repositories"] as const,
	workspaces: ["workspaces"] as const,
	sessions: ["sessions"] as const,
	sessionThreads: (sessionId: string, scope = "local") =>
		["sessionThreads", scope, sessionId] as const,
	sessionSearch: (query: string, scope = "local") =>
		["sessionSearch", scope, query] as const,
	workspaceSessions: (workspaceId: string, scope = "local") =>
		["workspaceSessions", scope, workspaceId] as const,
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

	const invalidateCoreEventQueries = (payload: unknown) => {
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.repositories });
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.workspaces });
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.sessions });
		void queryClient.invalidateQueries({ queryKey: ["sessionThreads"] });
		void queryClient.invalidateQueries({ queryKey: ["sessionSearch"] });
		void queryClient.invalidateQueries({ queryKey: ["workspaceSessions"] });
		if (shouldRefreshInspectorGitQueries(payload)) {
			void queryClient.invalidateQueries({
				predicate: (query) => {
					const queryKey = query.queryKey[0];
					return (
						queryKey === "workspaceGitStatus" ||
						queryKey === "workspacePrStatus" ||
						queryKey === "workspacePipeline" ||
						queryKey === "workspaceDeliveryFailureSnapshot" ||
						queryKey === "workspaceReviewState" ||
						queryKey === "workspaceGitBranchDiff"
					);
				},
			});
		}
	};

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
			invalidateCoreEventQueries(event.payload);
		});
	});
	if (typeof window !== "undefined") {
		window.addEventListener(REMOTE_CORE_EVENT_NAME, (event) => {
			invalidateCoreEventQueries((event as CustomEvent).detail);
		});
	}

	return queryClient;
}

export const dccQueryPersister = createAsyncStoragePersister({
	key: DCC_QUERY_CACHE_STORAGE_KEY,
	storage: browserStorage
		? createDccQueryCacheStorage(browserStorage)
		: undefined,
	serialize: serializeDccQueryCache,
	retry: removeOldestQuery,
});

export const dccQueryPersistOptions = {
	persister: dccQueryPersister,
	buster: DCC_QUERY_CACHE_BUSTER,
	maxAge: 24 * 60 * 60_000,
	dehydrateOptions: {
		shouldDehydrateMutation: () => false,
		shouldDehydrateQuery: shouldPersistDccQuery,
	},
};
