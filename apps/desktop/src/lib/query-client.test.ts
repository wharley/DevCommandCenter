import { dehydrate, QueryClient } from "@tanstack/react-query";
import type { PersistedClient } from "@tanstack/react-query-persist-client";
import { describe, expect, it, vi } from "vitest";
import {
	compactDccPersistedClient,
	createDccQueryCacheStorage,
	DCC_QUERY_CACHE_BUSTER,
	DCC_QUERY_CACHE_MAX_CHARS,
	DCC_QUERY_CACHE_STORAGE_KEY,
	prepareDccQueryCacheStorage,
	serializeDccQueryCache,
	shouldPersistDccQuery,
} from "./query-client";

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
