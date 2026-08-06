import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import {
	cleanupCompletedWorkspaceFrontendState,
	cleanupDeletedWorkspaceFrontendState,
} from "./frontend-state-cleanup";

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
});
