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
import type { CoreEvent, WorkspaceSessionSummary } from "@dcc/contracts";
import type { WorkspaceSummary } from "@/features/workspaces/types";
import { REMOTE_CORE_EVENT_NAME } from "./session-api";

export const DCC_QUERY_CACHE_STORAGE_KEY = "dcc-query-cache";
export const DCC_QUERY_CACHE_BUSTER = "dcc-query-cache-v3";
export const WORKSPACE_FORGE_METADATA_UPDATED_EVENT =
	"dcc/workspace/forge-metadata-updated";

export type WorkspaceForgeMetadataUpdatedPayload = {
	workspaceId: string;
	workspaceRoot: string;
};

export const DCC_QUERY_GC_TIME_MS = {
	/** Small shell metadata that makes remounts/navigation instant. */
	metadata: 24 * 60 * 60_000,
	/** Normal inactive data should not occupy the WebView for an entire day. */
	default: 15 * 60_000,
	/** Session event histories are reloadable from SQLite, but useful when revisiting. */
	history: 5 * 60_000,
	/** File bodies, diffs and logs can be very large and are cheap to read again. */
	heavyPayload: 2 * 60_000,
	/** Search results are ephemeral and should disappear soon after closing the dialog. */
	search: 60_000,
} as const;

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

type DccCoreEventRefreshPlan = {
	workspaceId: string | null;
	sessionId: string | null;
	refreshCollections: boolean;
	refreshSessionMetadata: boolean;
	refreshThreadHistory: boolean;
	refreshInspectorGit: boolean;
};

const EMPTY_CORE_EVENT_REFRESH_PLAN: DccCoreEventRefreshPlan = {
	workspaceId: null,
	sessionId: null,
	refreshCollections: false,
	refreshSessionMetadata: false,
	refreshThreadHistory: false,
	refreshInspectorGit: false,
};

/**
 * Streaming deltas are already delivered to the live event feed. Invalidating
 * SQLite-backed queries for every token/tool chunk causes an avoidable refetch
 * storm and repeatedly materializes an ever-growing event history.
 */
export function coreEventRefreshPlan(event: CoreEvent): DccCoreEventRefreshPlan {
	if ("workspacePrepared" in event && event.workspacePrepared) {
		return {
			...EMPTY_CORE_EVENT_REFRESH_PLAN,
			workspaceId: event.workspacePrepared.workspace_id,
			refreshCollections: true,
			refreshInspectorGit: true,
		};
	}
	if ("workspaceReady" in event && event.workspaceReady) {
		return {
			...EMPTY_CORE_EVENT_REFRESH_PLAN,
			workspaceId: event.workspaceReady.workspace_id,
			refreshCollections: true,
			refreshInspectorGit: true,
		};
	}
	if ("sessionStarted" in event && event.sessionStarted) {
		return {
			...EMPTY_CORE_EVENT_REFRESH_PLAN,
			workspaceId: event.sessionStarted.workspace_id,
			sessionId: event.sessionStarted.session_id,
			refreshSessionMetadata: true,
			refreshThreadHistory: true,
		};
	}

	const lifecycle =
		("sessionCompleted" in event && event.sessionCompleted) ||
		("sessionAborted" in event && event.sessionAborted) ||
		("sessionResumed" in event && event.sessionResumed) ||
		("sessionTurnStarted" in event && event.sessionTurnStarted) ||
		("sessionTurnCompleted" in event && event.sessionTurnCompleted) ||
		("sessionTurnAborted" in event && event.sessionTurnAborted) ||
		null;
	if (lifecycle) {
		return {
			...EMPTY_CORE_EVENT_REFRESH_PLAN,
			sessionId: lifecycle.session_id,
			refreshSessionMetadata: true,
			refreshThreadHistory: true,
			refreshInspectorGit: Boolean(
				("sessionCompleted" in event && event.sessionCompleted) ||
					("sessionTurnCompleted" in event && event.sessionTurnCompleted) ||
					("sessionTurnAborted" in event && event.sessionTurnAborted),
			),
		};
	}

	const toolSettled =
		("sessionTurnToolCallCompleted" in event && event.sessionTurnToolCallCompleted) ||
		("sessionTurnToolCallFailed" in event && event.sessionTurnToolCallFailed) ||
		null;
	if (toolSettled) {
		return {
			...EMPTY_CORE_EVENT_REFRESH_PLAN,
			sessionId: toolSettled.session_id,
			refreshInspectorGit: true,
		};
	}

	return EMPTY_CORE_EVENT_REFRESH_PLAN;
}

function workspaceIdForSession(queryClient: QueryClient, sessionId: string): string | null {
	for (const query of queryClient.getQueryCache().findAll({ queryKey: ["workspaceSessions"] })) {
		const summaries = query.state.data as WorkspaceSessionSummary[] | undefined;
		const match = summaries?.find((summary) => summary.session.id === sessionId);
		if (match) return match.session.workspaceId;
	}
	return null;
}

function workspaceRoots(queryClient: QueryClient, workspaceId: string | null): Set<string> {
	const roots = new Set<string>();
	if (!workspaceId) return roots;
	for (const query of queryClient.getQueryCache().findAll({ queryKey: ["workspaces"] })) {
		const workspaces = query.state.data as WorkspaceSummary[] | undefined;
		const workspace = workspaces?.find((candidate) => candidate.id === workspaceId);
		for (const root of [workspace?.worktreePath, workspace?.rootPath]) {
			if (root?.trim()) roots.add(root.trim());
		}
	}
	return roots;
}

const gitRefreshTimers = new WeakMap<
	QueryClient,
	Map<string, ReturnType<typeof setTimeout>>
>();

export function applyCoreEventQueryRefresh(
	queryClient: QueryClient,
	event: CoreEvent,
	input: { gitDebounceMs?: number } = {},
) {
	const plan = coreEventRefreshPlan(event);
	if (plan === EMPTY_CORE_EVENT_REFRESH_PLAN) return plan;
	const workspaceId =
		plan.workspaceId ??
		(plan.sessionId ? workspaceIdForSession(queryClient, plan.sessionId) : null);

	if (plan.refreshCollections) {
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.repositories });
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.workspaces });
	}
	if (plan.refreshSessionMetadata) {
		void queryClient.invalidateQueries({ queryKey: dccQueryKeys.sessions });
		if (workspaceId) {
			void queryClient.invalidateQueries({
				queryKey: ["workspaceSessions"],
				predicate: (query) => query.queryKey[2] === workspaceId,
			});
		} else {
			// Cache may be cold or already compacted. Lifecycle events are infrequent,
			// so a family fallback is safer than leaving shell metadata stale.
			void queryClient.invalidateQueries({ queryKey: ["workspaceSessions"] });
		}
	}
	if (plan.refreshThreadHistory && plan.sessionId) {
		void queryClient.invalidateQueries({
			queryKey: ["sessionThreads"],
			predicate: (query) => query.queryKey[2] === plan.sessionId,
		});
	}
	if (plan.refreshInspectorGit) {
		const timers = gitRefreshTimers.get(queryClient) ?? new Map();
		gitRefreshTimers.set(queryClient, timers);
		const timerKey = workspaceId ?? plan.sessionId ?? "__global__";
		const existing = timers.get(timerKey);
		if (existing) clearTimeout(existing);
		const roots = workspaceRoots(queryClient, workspaceId);
		const timer = setTimeout(() => {
			timers.delete(timerKey);
			void queryClient.invalidateQueries({
				predicate: (query) => {
					const root = query.queryKey[0];
					const queryRoot = query.queryKey[1];
					return (
						(root === "workspaceGitStatus" ||
							root === "workspacePrStatus" ||
							root === "workspacePipeline" ||
							root === "workspaceDeliveryFailureSnapshot" ||
							root === "workspaceReviewState" ||
							root === "workspaceGitBranchDiff") &&
						(roots.size === 0 ||
							(typeof queryRoot === "string" && roots.has(queryRoot)))
					);
				},
			});
		}, input.gitDebounceMs ?? 200);
		timers.set(timerKey, timer);
	}
	return { ...plan, workspaceId };
}

export function applyWorkspaceForgeMetadataUpdated(
	queryClient: QueryClient,
	payload: WorkspaceForgeMetadataUpdatedPayload,
) {
	const root = payload.workspaceRoot?.trim();
	void queryClient.invalidateQueries({ queryKey: ["repositories"] });
	void queryClient.invalidateQueries({ queryKey: ["workspaces"] });
	if (root) {
		void queryClient.invalidateQueries({
			queryKey: ["workspaceForgeContext", root],
		});
	}
}

export function configureDccQueryGcDefaults(queryClient: QueryClient) {
	// Query defaults merge from generic to specific prefixes. Keep compact shell
	// metadata warm, while allowing reloadable payloads to leave the JS heap soon
	// after their final observer unmounts. Persistence remains governed separately
	// by shouldPersistDccQuery and the 1 MB serialized snapshot budget above.
	for (const root of ["shell", "repositories", "workspaces", "providers"]) {
		queryClient.setQueryDefaults([root], {
			gcTime: DCC_QUERY_GC_TIME_MS.metadata,
		});
	}
	queryClient.setQueryDefaults(["sessionThreads"], {
		gcTime: DCC_QUERY_GC_TIME_MS.history,
	});
	for (const queryKey of [
		["workspaceFileContent"],
		["workspaceGitFilePreviewContent"],
		["workspaceGitBranchDiff"],
		["workspacePipelineJobLog"],
		["pullRequestHub", "detailCode"],
	] as const) {
		queryClient.setQueryDefaults(queryKey, {
			gcTime: DCC_QUERY_GC_TIME_MS.heavyPayload,
		});
	}
	for (const root of ["sessionSearch", "workspaceSearch"]) {
		queryClient.setQueryDefaults([root], {
			gcTime: DCC_QUERY_GC_TIME_MS.search,
		});
	}
	return queryClient;
}

export function createDccQueryClient() {
	const queryClient = configureDccQueryGcDefaults(
		new QueryClient({
			defaultOptions: {
				queries: {
					gcTime: DCC_QUERY_GC_TIME_MS.default,
					retry: 1,
					refetchOnWindowFocus: true,
					refetchOnReconnect: false,
				},
			},
		}),
	);

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
			applyCoreEventQueryRefresh(queryClient, event.payload as CoreEvent);
		});
		void listen<WorkspaceForgeMetadataUpdatedPayload>(
			WORKSPACE_FORGE_METADATA_UPDATED_EVENT,
			(event) => applyWorkspaceForgeMetadataUpdated(queryClient, event.payload),
		);
	});
	if (typeof window !== "undefined") {
		window.addEventListener(REMOTE_CORE_EVENT_NAME, (event) => {
			applyCoreEventQueryRefresh(
				queryClient,
				(event as CustomEvent<CoreEvent>).detail,
			);
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
