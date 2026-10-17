import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import {
	cleanupCompletedWorkspaceFrontendState,
	cleanupDeletedWorkspaceFrontendState,
	removeDeletedWorkspaceStorage,
	removeLegacyStorage,
} from "./frontend-state-cleanup";

function memoryStorage(entries: Record<string, string>): Storage {
	const values = new Map(Object.entries(entries));
	return {
		get length() {
			return values.size;
		},
		clear: () => values.clear(),
		getItem: (key) => values.get(key) ?? null,
		key: (index) => [...values.keys()][index] ?? null,
		removeItem: (key) => void values.delete(key),
		setItem: (key, value) => void values.set(key, value),
	};
}

describe("frontend state cleanup", () => {
	it("is exact, idempotent, and keeps durable history on completion", () => {
		const client = new QueryClient();
		client.setQueryData(["sessionThreads", "local", "s1"], [1]);
		client.setQueryData(["workspaceFileContent", "/repo/a", "x"], "a");
		client.setQueryData(["workspaceFileContent", "/repo/b", "x"], "b");
		cleanupCompletedWorkspaceFrontendState(client, { workspaceIds: ["w1"], roots: ["/repo/a"] });
		expect(client.getQueryData(["sessionThreads", "local", "s1"])).toEqual([1]);
		expect(client.getQueryData(["workspaceFileContent", "/repo/a", "x"])).toBeUndefined();
		expect(client.getQueryData(["workspaceFileContent", "/repo/b", "x"])).toBe("b");
	});

	it("deletes all scopes for only the affected sessions/workspace", () => {
		const client = new QueryClient();
		for (const scope of ["local", "remote"]) client.setQueryData(["sessionThreads", scope, "s1"], []);
		client.setQueryData(["sessionThreads", "local", "s2"], []);
		client.setQueryData(["workspaceSessions", "local", "w1"], []);
		client.setQueryData(["workspaceSessions", "local", "w2"], []);
		const input = {
			workspaceIds: ["w1"],
			sessionIds: ["s1"],
			roots: ["/repo/a"],
		};
		cleanupDeletedWorkspaceFrontendState(client, input);
		cleanupDeletedWorkspaceFrontendState(client, input);
		expect(client.getQueryData(["sessionThreads", "local", "s1"])).toBeUndefined();
		expect(client.getQueryData(["sessionThreads", "remote", "s1"])).toBeUndefined();
		expect(client.getQueryData(["sessionThreads", "local", "s2"])).toEqual([]);
		expect(client.getQueryData(["workspaceSessions", "local", "w1"])).toBeUndefined();
		expect(client.getQueryData(["workspaceSessions", "local", "w2"])).toEqual([]);
	});

	it("normalizes Windows paths without crossing a sibling boundary", () => {
		const client = new QueryClient();
		client.setQueryData(
			["workspaceFileContent", "C:\\repo\\a", "one.ts"],
			"one",
		);
		client.setQueryData(
			["workspaceFileContent", "C:\\repo\\ab", "two.ts"],
			"two",
		);
		cleanupCompletedWorkspaceFrontendState(client, {
			workspaceIds: [],
			roots: ["C:/repo/a/"],
		});
		expect(
			client.getQueryData(["workspaceFileContent", "C:\\repo\\a", "one.ts"]),
		).toBeUndefined();
		expect(
			client.getQueryData(["workspaceFileContent", "C:\\repo\\ab", "two.ts"]),
		).toBe("two");
	});

	it("removes only the deleted task's composer keys and session picks", () => {
		const storage = memoryStorage({
			"dcc.workspace.composer.draft.w1": "draft",
			"dcc.workspace.composer.draft.w1.conversation.new": "new",
			"dcc.workspace.composer.draft.w1.conversation.session.s1.editor.v1": "{}",
			"dcc.workspace.composer.effort.w1": "high",
			"dcc.workspace.composer.approval.w1.codex": "auto",
			"dcc.workspace.composer.draft.w10": "sibling",
			"dcc.workspace.composer.effort.w2": "low",
			"dcc.theme": "dark",
			"dcc.sessionComposerSelections": JSON.stringify({
				s1: { providerId: "codex", modelId: "gpt" },
				s2: { providerId: "claude", modelId: "opus" },
			}),
		});

		removeDeletedWorkspaceStorage(storage, { workspaceIds: ["w1"], sessionIds: ["s1"] });

		const remaining = Array.from({ length: storage.length }, (_, index) => storage.key(index));
		expect(remaining.sort()).toEqual([
			"dcc.sessionComposerSelections",
			"dcc.theme",
			"dcc.workspace.composer.draft.w10",
			"dcc.workspace.composer.effort.w2",
		]);
		expect(JSON.parse(storage.getItem("dcc.sessionComposerSelections") ?? "{}")).toEqual({
			s2: { providerId: "claude", modelId: "opus" },
		});
	});

	it("drops keys written by removed features", () => {
		const storage = memoryStorage({
			"dcc:attention:records": "[]",
			"dcc-review-trail-dcc-1::primary": "[]",
			"dcc-terminal-tabs-v1": "{}",
		});

		removeLegacyStorage(storage);

		expect(storage.length).toBe(1);
		expect(storage.getItem("dcc-terminal-tabs-v1")).toBe("{}");
	});
});
