import assert from "node:assert/strict";
import test from "node:test";

import {
	dccMcpQueryOptions,
	failedDccMcpStatus,
	normalizeDccMcpServers,
	readDccMcpStatus,
	resolveDccMcpToolPolicy,
} from "./mcp-config.mjs";
import { waitForDccMcpReadiness } from "./mcp-readiness.mjs";

test("normalizes DCC-owned stdio and HTTP servers for the Agent SDK", () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "command-fixture",
			name: "dcc-command-fixture",
			transport: {
				type: "stdio",
				command: "/absolute/fixture",
				args: ["stdio"],
				env: { FIXTURE_TOKEN: "secret-canary" },
			},
		},
		{
			definitionId: "http-fixture",
			name: "dcc-http-fixture",
			transport: {
				type: "http",
				url: "http://127.0.0.1:8765/mcp",
				headers: { Authorization: "Bearer secret-canary" },
			},
		},
	]);

	assert.deepEqual(Object.keys(projection.servers), [
		"dcc-command-fixture",
		"dcc-http-fixture",
	]);
	assert.equal(projection.servers["dcc-command-fixture"].alwaysLoad, true);
	assert.equal(projection.servers["dcc-http-fixture"].type, "http");
});

test("waits through pending MCP status before releasing a connected snapshot", async () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "remote",
			name: "dcc-remote",
			transport: {
				type: "http",
				url: "https://mcp.example.com/mcp",
				headers: {},
			},
		},
	]);
	const statuses = [
		[{ name: "dcc-remote", status: "pending", tools: [] }],
		[
			{
				name: "dcc-remote",
				status: "connected",
				tools: [{ name: "task.get" }],
			},
		],
	];
	const snapshots = [];
	let clock = 0;
	const result = await waitForDccMcpReadiness(
		{
			async mcpServerStatus() {
				return statuses.shift();
			},
		},
		projection,
		{
			timeoutMs: 1_000,
			pollIntervalMs: 50,
			onSnapshot: (snapshot) => snapshots.push(snapshot),
			now: () => clock,
			wait: async (milliseconds) => {
				clock += milliseconds;
			},
		},
	);

	assert.equal(snapshots.length, 2);
	assert.equal(snapshots[0].servers[0].status, "pending");
	assert.equal(result.servers[0].status, "connected");
	assert.deepEqual(result.servers[0].tools, [
		{ name: "task.get", annotations: {} },
	]);
});

test("fails closed when MCP attachment remains pending until timeout", async () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "remote",
			name: "dcc-remote",
			transport: {
				type: "http",
				url: "https://mcp.example.com/mcp",
				headers: {},
			},
		},
	]);
	let clock = 0;
	const result = await waitForDccMcpReadiness(
		{
			async mcpServerStatus() {
				return [{ name: "dcc-remote", status: "pending", tools: [] }];
			},
		},
		projection,
		{
			timeoutMs: 100,
			pollIntervalMs: 50,
			now: () => clock,
			wait: async (milliseconds) => {
				clock += milliseconds;
			},
		},
	);

	assert.deepEqual(result.failed, ["dcc-remote"]);
	assert.equal(result.servers[0].status, "failed");
});

test("resolves only explicit policies for DCC-owned MCP tools", () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "fixture",
			name: "dcc-fixture",
			transport: {
				type: "http",
				url: "https://example.com/mcp",
				headers: {},
			},
			toolPolicies: [
				{ toolName: "read", decision: "allow" },
				{ toolName: "mutate", decision: "deny" },
			],
		},
	]);

	assert.deepEqual(
		resolveDccMcpToolPolicy(projection, "mcp__dcc-fixture__read"),
		{ decision: "allow", toolName: "read" },
	);
	assert.deepEqual(
		resolveDccMcpToolPolicy(projection, "mcp__dcc-fixture__unknown"),
		{ decision: "ask", toolName: "unknown" },
	);
	assert.equal(resolveDccMcpToolPolicy(projection, "Bash"), null);
	assert.deepEqual(dccMcpQueryOptions(projection).allowedTools, [
		"mcp__dcc-fixture__read",
	]);
	assert.deepEqual(dccMcpQueryOptions(projection).disallowedTools, [
		"mcp__dcc-fixture__mutate",
	]);
});

test("rejects names outside the DCC namespace and header injection", () => {
	assert.throws(
		() =>
			normalizeDccMcpServers([
				{
					definitionId: "fixture",
					name: "user-owned-name",
					transport: {
						type: "http",
						url: "https://example.com/mcp",
						headers: {},
					},
				},
			]),
		/invalid DCC MCP configuration/,
	);
	assert.throws(
		() =>
			normalizeDccMcpServers([
				{
					definitionId: "fixture",
					name: "dcc-fixture",
					transport: {
						type: "http",
						url: "https://example.com/mcp",
						headers: { Authorization: "ok\r\nX-Evil: yes" },
					},
				},
			]),
		/invalid DCC MCP configuration/,
	);
});

test("builds documented query options and returns only bounded DCC status metadata", async () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "fixture",
			name: "dcc-fixture",
			transport: {
				type: "http",
				url: "https://example.com/mcp",
				headers: {},
			},
		},
	]);
	const query = {
		async mcpServerStatus() {
			return [
				{
					name: "dcc-fixture",
					status: "connected",
					tools: [
						{
							name: "fixture.echo",
							description: "not forwarded",
							annotations: {
								readOnly: true,
								destructive: false,
								openWorld: false,
								untrustedText: "not forwarded",
							},
						},
					],
				},
				{
					name: "user-configured-server",
					status: "connected",
					tools: [{ name: "private.tool" }],
				},
			];
		},
	};

	const options = dccMcpQueryOptions(projection);
	const status = await readDccMcpStatus(query, projection);

	assert.equal(options.mcpServers, projection.servers);
	assert.deepEqual(status.servers, [
		{
			definitionId: "fixture",
			name: "dcc-fixture",
			status: "connected",
			tools: [
				{
					name: "fixture.echo",
					annotations: {
						readOnlyHint: true,
						destructiveHint: false,
						openWorldHint: false,
					},
				},
			],
		},
	]);
	assert.equal(JSON.stringify(status).includes("not forwarded"), false);
	assert.equal(JSON.stringify(status).includes("private.tool"), false);
});

test("an empty DCC projection leaves provider-configured servers untouched", async () => {
	let called = false;
	const query = {
		async mcpServerStatus() {
			called = true;
		},
	};
	const projection = normalizeDccMcpServers([]);

	const options = dccMcpQueryOptions(projection);
	const result = await readDccMcpStatus(query, projection);

	assert.equal(called, false);
	assert.deepEqual(options, {});
	assert.deepEqual(result, {
		failed: [],
		servers: [],
	});
});

test("keeps every projected server in a deterministic fail-closed snapshot", async () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "connected",
			name: "dcc-connected",
			transport: {
				type: "http",
				url: "https://example.com/connected",
				headers: {},
			},
		},
		{
			definitionId: "missing",
			name: "dcc-missing",
			transport: {
				type: "http",
				url: "https://example.com/missing",
				headers: {},
			},
		},
		{
			definitionId: "duplicate",
			name: "dcc-duplicate",
			transport: {
				type: "http",
				url: "https://example.com/duplicate",
				headers: {},
			},
		},
	]);
	const query = {
		async mcpServerStatus() {
			return [
				{
					name: "dcc-connected",
					status: "connected",
					tools: [
						{ name: "fixture.echo" },
						{ name: "fixture.echo" },
						{ name: "invalid tool name" },
					],
				},
				{ name: "dcc-duplicate", status: "connected", tools: [] },
				{ name: "dcc-duplicate", status: "connected", tools: [] },
			];
		},
	};

	const status = await readDccMcpStatus(query, projection);

	assert.deepEqual(status.servers, [
		{
			definitionId: "connected",
			name: "dcc-connected",
			status: "connected",
			tools: [{ name: "fixture.echo", annotations: {} }],
		},
		{
			definitionId: "missing",
			name: "dcc-missing",
			status: "pending",
			tools: [],
		},
		{
			definitionId: "duplicate",
			name: "dcc-duplicate",
			status: "failed",
			tools: [],
		},
	]);
	assert.deepEqual(status.failed, ["dcc-duplicate"]);
	assert.deepEqual(failedDccMcpStatus(projection).failed, [
		"dcc-connected",
		"dcc-missing",
		"dcc-duplicate",
	]);
	assert.deepEqual(
		(
			await readDccMcpStatus(
				{
					async mcpServerStatus() {
						return { invalid: true };
					},
				},
				projection,
			)
		).failed,
		["dcc-connected", "dcc-missing", "dcc-duplicate"],
	);
});

test("only DCC's own HTTP server gets the long tool timeout", async () => {
	const { normalizeDccMcpServers, DCC_INTERNAL_TOOL_TIMEOUT_MS } = await import("./mcp-config.mjs");
	const projection = normalizeDccMcpServers([
		{
			definitionId: "dcc-browser-webview-internal",
			name: "dcc-internal",
			transport: { type: "http", url: "http://127.0.0.1:1234/mcp", headers: {} },
		},
		{
			definitionId: "user-http",
			name: "dcc-user",
			transport: { type: "http", url: "https://example.test/mcp", headers: {} },
		},
	]);
	assert.equal(projection.servers["dcc-internal"].timeout, DCC_INTERNAL_TOOL_TIMEOUT_MS);
	assert.equal(projection.servers["dcc-user"].timeout, undefined);
});
