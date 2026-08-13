import { dehydrate, QueryClient } from "@tanstack/react-query";
import type { PersistedClient } from "@tanstack/react-query-persist-client";
import type { CoreEvent, WorkspaceSessionSummary } from "@dcc/contracts";
import { describe, expect, it, vi } from "vitest";
import {
	compactDccPersistedClient,
	applyCoreEventQueryRefresh,
	applyWorkspaceForgeMetadataUpdated,
	configureDccQueryGcDefaults,
	createDccQueryCacheStorage,
	DCC_QUERY_CACHE_BUSTER,
	DCC_QUERY_CACHE_MAX_CHARS,
	DCC_QUERY_CACHE_STORAGE_KEY,
	DCC_QUERY_GC_TIME_MS,
	prepareDccQueryCacheStorage,
	serializeDccQueryCache,
	shouldPersistDccQuery,
} from "./query-client";

function coreEvent(value: object): CoreEvent {
	return value as CoreEvent;
}

class MemoryStorage implements Storage {
	readonly values = new Map<string, string>();

	get length() {
		return this.values.size;
	}

	clear() {
		this.values.clear();
	}

	getItem(key: string) {
		return this.values.get(key) ?? null;
	}

	key(index: number) {
		return [...this.values.keys()][index] ?? null;
	}

	removeItem(key: string) {
		this.values.delete(key);
	}

	setItem(key: string, value: string) {
		this.values.set(key, value);
	}
}

function persistedClient(queryClient: QueryClient): PersistedClient {
	return {
		buster: DCC_QUERY_CACHE_BUSTER,
		timestamp: Date.now(),
		clientState: dehydrate(queryClient),
	};
}

describe("DCC query cache persistence", () => {
	it("refreshes only the forge context for the workspace whose binding settled", async () => {
		const queryClient = new QueryClient();
		queryClient.setQueryData(["repositories", "local"], []);
		queryClient.setQueryData(["workspaces", "local"], []);
		queryClient.setQueryData(["workspaceForgeContext", "/repo/a", ""], {});
		queryClient.setQueryData(["workspaceForgeContext", "/repo/b", ""], {});

		applyWorkspaceForgeMetadataUpdated(queryClient, {
			workspaceId: "workspace-a",
			workspaceRoot: "/repo/a",
		});
		await Promise.resolve();

		expect(queryClient.getQueryState(["repositories", "local"])?.isInvalidated).toBe(true);
		expect(queryClient.getQueryState(["workspaces", "local"])?.isInvalidated).toBe(true);
		expect(
			queryClient.getQueryState(["workspaceForgeContext", "/repo/a", ""])
				?.isInvalidated,
		).toBe(true);
		expect(
			queryClient.getQueryState(["workspaceForgeContext", "/repo/b", ""])
				?.isInvalidated,
		).toBe(false);
	});

	it("does not invalidate any query for 1000 streaming deltas", () => {
		const queryClient = new QueryClient();
		const invalidate = vi.spyOn(queryClient, "invalidateQueries");

		for (let index = 0; index < 1000; index += 1) {
			applyCoreEventQueryRefresh(
				queryClient,
				coreEvent({
					sessionTurnAssistantMessageDelta: {
						session_id: "session-a",
						turn_id: "turn-a",
						message_id: "message-a",
						content: "x",
					},
				}),
			);
		}

		expect(invalidate).not.toHaveBeenCalled();
	});

	it("targets completion refreshes and debounces Git queries", async () => {
		vi.useFakeTimers();
		const queryClient = new QueryClient();
		queryClient.setQueryData(
			["workspaceSessions", "local", "workspace-a"],
			[{ session: { id: "session-a", workspaceId: "workspace-a" } }] as WorkspaceSessionSummary[],
		);
		queryClient.setQueryData(
			["workspaceSessions", "local", "workspace-b"],
			[{ session: { id: "session-b", workspaceId: "workspace-b" } }] as WorkspaceSessionSummary[],
		);
		queryClient.setQueryData(["workspaces", "local"], [
			{ id: "workspace-a", worktreePath: "/repo/a" },
			{ id: "workspace-b", worktreePath: "/repo/b" },
		]);
		queryClient.setQueryData(["sessionThreads", "local", "session-a"], []);
		queryClient.setQueryData(["sessionThreads", "local", "session-b"], []);
		queryClient.setQueryData(["workspaceGitStatus", "/repo/a"], {});
		queryClient.setQueryData(["workspaceGitStatus", "/repo/b"], {});

		applyCoreEventQueryRefresh(
			queryClient,
			coreEvent({ sessionTurnCompleted: { session_id: "session-a", turn_id: "turn-a" } }),
			{ gitDebounceMs: 200 },
		);
		await Promise.resolve();

		expect(
			queryClient.getQueryState(["sessionThreads", "local", "session-a"])
				?.isInvalidated,
		).toBe(true);
		expect(
			queryClient.getQueryState(["sessionThreads", "local", "session-b"])
				?.isInvalidated,
		).toBe(false);
		expect(
			queryClient.getQueryState(["workspaceSessions", "local", "workspace-a"])
				?.isInvalidated,
		).toBe(true);
		expect(
			queryClient.getQueryState(["workspaceSessions", "local", "workspace-b"])
				?.isInvalidated,
		).toBe(false);
		expect(queryClient.getQueryState(["workspaceGitStatus", "/repo/a"])?.isInvalidated).toBe(false);

		await vi.advanceTimersByTimeAsync(200);
		expect(queryClient.getQueryState(["workspaceGitStatus", "/repo/a"])?.isInvalidated).toBe(true);
		expect(queryClient.getQueryState(["workspaceGitStatus", "/repo/b"])?.isInvalidated).toBe(false);
		vi.useRealTimers();
	});

	it("falls back to query families when lifecycle ownership is not cached", async () => {
		vi.useFakeTimers();
		const queryClient = new QueryClient();
		queryClient.setQueryData(["workspaceSessions", "local", "workspace-a"], []);
		queryClient.setQueryData(["workspaceSessions", "local", "workspace-b"], []);
		queryClient.setQueryData(["workspaceGitStatus", "/repo/a"], {});
		queryClient.setQueryData(["workspaceGitStatus", "/repo/b"], {});

		applyCoreEventQueryRefresh(
			queryClient,
			coreEvent({
				sessionTurnCompleted: { session_id: "unknown", turn_id: "turn-a" },
			}),
			{ gitDebounceMs: 200 },
		);
		await Promise.resolve();
		expect(
			queryClient.getQueryState(["workspaceSessions", "local", "workspace-a"])
				?.isInvalidated,
		).toBe(true);
		expect(
			queryClient.getQueryState(["workspaceSessions", "local", "workspace-b"])
				?.isInvalidated,
		).toBe(true);

		await vi.advanceTimersByTimeAsync(200);
		expect(
			queryClient.getQueryState(["workspaceGitStatus", "/repo/a"])?.isInvalidated,
		).toBe(true);
		expect(
			queryClient.getQueryState(["workspaceGitStatus", "/repo/b"])?.isInvalidated,
		).toBe(true);
		vi.useRealTimers();
	});

	it("keeps metadata warm but quickly collects reloadable heavy payloads", () => {
		const queryClient = configureDccQueryGcDefaults(
			new QueryClient({
				defaultOptions: {
					queries: { gcTime: DCC_QUERY_GC_TIME_MS.default },
				},
			}),
		);

		expect(queryClient.getDefaultOptions().queries?.gcTime).toBe(
			DCC_QUERY_GC_TIME_MS.default,
		);
		expect(queryClient.getQueryDefaults(["workspaces", "local"]).gcTime).toBe(
			DCC_QUERY_GC_TIME_MS.metadata,
		);
		expect(
			queryClient.getQueryDefaults(["sessionThreads", "local", "session-1"])
				.gcTime,
		).toBe(DCC_QUERY_GC_TIME_MS.history);
		expect(
			queryClient.getQueryDefaults(["workspaceFileContent", "/repo", "large.ts"])
				.gcTime,
		).toBe(DCC_QUERY_GC_TIME_MS.heavyPayload);
		expect(
			queryClient.getQueryDefaults(["pullRequestHub", "detailCode", "pr-1"])
				.gcTime,
		).toBe(DCC_QUERY_GC_TIME_MS.heavyPayload);
		expect(queryClient.getQueryDefaults(["sessionSearch", "term"]).gcTime).toBe(
			DCC_QUERY_GC_TIME_MS.search,
		);
	});

	it("persists shell metadata and thread histories but excludes file payloads", () => {
		const queryClient = new QueryClient();
		queryClient.setQueryData(["workspaces", "local"], [{ id: "workspace-1" }]);
		queryClient.setQueryData(["sessionThreads", "local", "session-1"], [
			{ payload: "recent event history" },
		]);
		queryClient.setQueryData(["workspaceFileContent", "/repo", "large.ts"], {
			content: "large file payload",
		});

		const workspaceQuery = queryClient.getQueryCache().find({
			queryKey: ["workspaces", "local"],
		});
		const threadQuery = queryClient.getQueryCache().find({
			queryKey: ["sessionThreads", "local", "session-1"],
		});
		const fileQuery = queryClient.getQueryCache().find({
			queryKey: ["workspaceFileContent", "/repo", "large.ts"],
		});

		expect(workspaceQuery && shouldPersistDccQuery(workspaceQuery)).toBe(true);
		expect(threadQuery && shouldPersistDccQuery(threadQuery)).toBe(true);
		expect(fileQuery && shouldPersistDccQuery(fileQuery)).toBe(false);
		expect(
			compactDccPersistedClient(persistedClient(queryClient)).clientState.queries.map(
				(query) => query.queryKey[0],
			),
		).toEqual(["workspaces", "sessionThreads"]);
	});

	it("keeps the serialized restart snapshot below its storage budget", () => {
		const queryClient = new QueryClient();
		for (let index = 0; index < 8; index += 1) {
			queryClient.setQueryData(["workspaceSessions", `workspace-${index}`], {
				id: index,
				payload: "x".repeat(220_000),
			});
		}

		const client = persistedClient(queryClient);
		const compacted = compactDccPersistedClient(client);
		const serialized = serializeDccQueryCache(client);

		expect(serialized.length).toBeLessThanOrEqual(DCC_QUERY_CACHE_MAX_CHARS);
		expect(compacted.clientState.queries.length).toBeGreaterThan(0);
		expect(compacted.clientState.queries.length).toBeLessThan(8);
		expect(compacted.clientState.mutations).toEqual([]);
	});

	it("removes only a legacy query snapshot during automatic migration", () => {
		const storage = new MemoryStorage();
		storage.setItem("dcc-theme", "dark");
		storage.setItem(
			DCC_QUERY_CACHE_STORAGE_KEY,
			JSON.stringify({ buster: "", timestamp: Date.now(), clientState: {} }),
		);

		prepareDccQueryCacheStorage(storage);

		expect(storage.getItem(DCC_QUERY_CACHE_STORAGE_KEY)).toBeNull();
		expect(storage.getItem("dcc-theme")).toBe("dark");
	});

	it("preserves a current bounded snapshot during startup", () => {
		const storage = new MemoryStorage();
		const current = JSON.stringify({
			buster: DCC_QUERY_CACHE_BUSTER,
			timestamp: Date.now(),
			clientState: { mutations: [], queries: [] },
		});
		storage.setItem(DCC_QUERY_CACHE_STORAGE_KEY, current);

		prepareDccQueryCacheStorage(storage);

		expect(storage.getItem(DCC_QUERY_CACHE_STORAGE_KEY)).toBe(current);
	});

	it("evicts only the query snapshot and retries once when WebKit reports quota", () => {
		const storage = new MemoryStorage();
		storage.setItem(DCC_QUERY_CACHE_STORAGE_KEY, "old-cache");
		storage.setItem("dcc-composer-draft", "keep-me");
		const originalSetItem = storage.setItem.bind(storage);
		let shouldFail = true;
		vi.spyOn(storage, "setItem").mockImplementation((key, value) => {
			if (key === DCC_QUERY_CACHE_STORAGE_KEY && shouldFail) {
				shouldFail = false;
				throw { name: "QuotaExceededError", code: 22 };
			}
			originalSetItem(key, value);
		});
		const resilientStorage = createDccQueryCacheStorage(storage);

		resilientStorage.setItem(DCC_QUERY_CACHE_STORAGE_KEY, "new-cache");

		expect(storage.getItem(DCC_QUERY_CACHE_STORAGE_KEY)).toBe("new-cache");
		expect(storage.getItem("dcc-composer-draft")).toBe("keep-me");
	});
});
