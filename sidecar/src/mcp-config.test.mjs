import assert from "node:assert/strict";
import test from "node:test";

import {
	dccMcpQueryOptions,
	normalizeDccMcpServers,
	readDccMcpStatus,
} from "./mcp-config.mjs";

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
					tools: [{ name: "fixture.echo", description: "not forwarded" }],
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
			tools: ["fixture.echo"],
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
