import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import {
	existsSync,
	readFileSync,
	readdirSync,
	statSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

import {
	dccMcpQueryOptions,
	failedDccMcpStatus,
	normalizeDccMcpServers,
	readDccMcpStatus,
	resolveDccMcpToolPolicy,
} from "./mcp-config.mjs";
import {
	createEphemeralMcpOAuthBridge,
	projectRemoteHttpServersThroughOAuthProxy,
} from "./mcp-oauth-bridge.mjs";
import {
	createDeferredUserPrompt,
	waitForDccMcpReadiness,
} from "./mcp-readiness.mjs";

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

test("projects remote HTTPS servers through the bundled ephemeral OAuth proxy", () => {
	const projection = normalizeDccMcpServers([
		{
			definitionId: "remote",
			name: "dcc-remote",
			transport: {
				type: "http",
				url: "https://mcp.example.com/mcp",
				headers: {
					Authorization: "Bearer secret-canary",
					"X-Tenant": "tenant-a",
				},
			},
		},
		{
			definitionId: "loopback",
			name: "dcc-loopback",
			transport: {
				type: "http",
				url: "http://127.0.0.1:8765/mcp",
				headers: {},
			},
		},
	]);

	const runtimeProjection = projectRemoteHttpServersThroughOAuthProxy(
		projection,
		{
			command: "/absolute/node",
			args: ["/absolute/sidecar.mjs", "--dcc-mcp-remote-proxy"],
			authConfigDir: "/private/session/oauth",
		},
	);
	const remote = runtimeProjection.servers["dcc-remote"];
	const authorizationIdentity = createHash("sha256")
		.update("remote")
		.update("\0")
		.update("authorization")
		.digest("hex")
		.slice(0, 24);
	const tenantIdentity = createHash("sha256")
		.update("remote")
		.update("\0")
		.update("x-tenant")
		.digest("hex")
		.slice(0, 24);

	assert.equal(remote.type, "stdio");
	assert.equal(remote.command, "/absolute/node");
	assert.deepEqual(remote.args, [
		"/absolute/sidecar.mjs",
		"--dcc-mcp-remote-proxy",
		"https://mcp.example.com/mcp",
		"--transport",
		"http-only",
		"--auth-timeout",
		"180",
		"--silent",
		"--header",
		`Authorization:\${DCC_MCP_REMOTE_HEADER_${authorizationIdentity}}`,
		"--header",
		`X-Tenant:\${DCC_MCP_REMOTE_HEADER_${tenantIdentity}}`,
	]);
	assert.deepEqual(remote.env, {
		MCP_REMOTE_CONFIG_DIR: "/private/session/oauth",
		DCC_MCP_REMOTE_STATE_DIR: "/private/session/oauth/state",
		[`DCC_MCP_REMOTE_HEADER_${authorizationIdentity}`]:
			"Bearer secret-canary",
		[`DCC_MCP_REMOTE_HEADER_${tenantIdentity}`]: "tenant-a",
	});
	assert.equal(JSON.stringify(remote.args).includes("secret-canary"), false);
	assert.equal(runtimeProjection.servers["dcc-loopback"].type, "http");
	assert.equal(runtimeProjection.definitionIds, projection.definitionIds);
	assert.equal(runtimeProjection.toolPolicies, projection.toolPolicies);
});

test("the pinned mcp-remote bundle writes into the DCC private state directory", async () => {
	const require = createRequire(import.meta.url);
	const packageJsonPath = require.resolve("mcp-remote/package.json");
	const packageDirectory = dirname(packageJsonPath);
	const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
	assert.equal(packageJson.version, "0.1.38");

	const chunks = readdirSync(join(packageDirectory, "dist"))
		.filter((name) => /^chunk-[A-Za-z0-9]+\.js$/.test(name))
		.map((name) => join(packageDirectory, "dist", name))
		.filter((path) =>
			readFileSync(path, "utf8").includes("function getConfigDir()"),
		);
	assert.equal(chunks.length, 1);

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
	const bridge = createEphemeralMcpOAuthBridge(import.meta.url);
	const runtimeProjection = bridge.project(projection);
	const stateDirectory =
		runtimeProjection.servers["dcc-remote"].env.DCC_MCP_REMOTE_STATE_DIR;
	const serverHash = createHash("md5")
		.update("https://mcp.example.com/mcp")
		.digest("hex");
	const previousStateDirectory = process.env.DCC_MCP_REMOTE_STATE_DIR;
	process.env.DCC_MCP_REMOTE_STATE_DIR = stateDirectory;
	try {
		const { NodeOAuthClientProvider } = await import(
			pathToFileURL(chunks[0]).href
		);
		const provider = new NodeOAuthClientProvider({
			serverUrl: "https://mcp.example.com/mcp",
			serverUrlHash: serverHash,
			host: "127.0.0.1",
			callbackPort: 3335,
		});
		await provider.saveTokens({
			access_token: "upstream-fixture-access-token",
			token_type: "bearer",
		});
		assert.equal(
			existsSync(join(stateDirectory, `${serverHash}_tokens.json`)),
			true,
		);
		assert.equal(bridge.collectUpdates().length, 1);
	} finally {
		if (previousStateDirectory === undefined) {
			delete process.env.DCC_MCP_REMOTE_STATE_DIR;
		} else {
			process.env.DCC_MCP_REMOTE_STATE_DIR = previousStateDirectory;
		}
		bridge.cleanup();
	}
});

test("keeps OAuth files private to the provider session and removes them", () => {
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
	const bridge = createEphemeralMcpOAuthBridge(import.meta.url);
	const runtimeProjection = bridge.project(projection);
	const authConfigDir =
		runtimeProjection.servers["dcc-remote"].env.MCP_REMOTE_CONFIG_DIR;
	const stateDirectory =
		runtimeProjection.servers["dcc-remote"].env.DCC_MCP_REMOTE_STATE_DIR;

	assert.equal(existsSync(authConfigDir), true);
	assert.equal(existsSync(stateDirectory), true);
	if (process.platform !== "win32") {
		assert.equal(statSync(authConfigDir).mode & 0o777, 0o700);
		assert.equal(statSync(stateDirectory).mode & 0o777, 0o700);
	}

	bridge.cleanup();
	assert.equal(existsSync(authConfigDir), false);
});

test("restores and captures provider-neutral OAuth state in the private session directory", () => {
	const initialState = {
		version: 1,
		clientInfo: {
			client_id: "public-client",
			redirect_uris: ["http://127.0.0.1:3335/oauth/callback"],
		},
		tokens: {
			access_token: "first-access-token",
			refresh_token: "first-refresh-token",
			token_type: "bearer",
		},
	};
	const serverUrl = "https://mcp.example.com/mcp";
	const projection = normalizeDccMcpServers([
		{
			definitionId: "remote",
			name: "dcc-remote",
			transport: {
				type: "http",
				url: serverUrl,
				headers: {},
			},
			oauthState: JSON.stringify(initialState),
		},
	]);
	const bridge = createEphemeralMcpOAuthBridge(import.meta.url);
	const runtimeProjection = bridge.project(projection);
	const authConfigDir =
		runtimeProjection.servers["dcc-remote"].env.MCP_REMOTE_CONFIG_DIR;
	const serverHash = createHash("md5").update(serverUrl).digest("hex");
	const stateDirectory =
		runtimeProjection.servers["dcc-remote"].env.DCC_MCP_REMOTE_STATE_DIR;
	const tokensPath = join(stateDirectory, `${serverHash}_tokens.json`);
	const clientInfoPath = join(
		stateDirectory,
		`${serverHash}_client_info.json`,
	);

	assert.deepEqual(JSON.parse(readFileSync(tokensPath, "utf8")), initialState.tokens);
	assert.deepEqual(
		JSON.parse(readFileSync(clientInfoPath, "utf8")),
		initialState.clientInfo,
	);
	if (process.platform !== "win32") {
		assert.equal(statSync(tokensPath).mode & 0o777, 0o600);
		assert.equal(statSync(clientInfoPath).mode & 0o777, 0o600);
	}
	assert.deepEqual(bridge.collectUpdates(), []);

	const refreshedTokens = {
		...initialState.tokens,
		access_token: "refreshed-access-token",
	};
	writeFileSync(tokensPath, JSON.stringify(refreshedTokens), { mode: 0o600 });
	const updates = bridge.collectUpdates();
	assert.equal(updates.length, 1);
	assert.equal(updates[0].definitionId, "remote");
	assert.deepEqual(JSON.parse(updates[0].state), {
		version: 1,
		clientInfo: initialState.clientInfo,
		tokens: refreshedTokens,
	});
	assert.deepEqual(bridge.collectUpdates(), []);

	unlinkSync(tokensPath);
	assert.deepEqual(bridge.collectUpdates(), [
		{ definitionId: "remote", state: null },
	]);
	assert.deepEqual(bridge.collectUpdates(), []);

	bridge.cleanup();
	assert.equal(existsSync(authConfigDir), false);
});

test("does not deliver the user prompt until MCP attachment is ready", async () => {
	const deferred = createDeferredUserPrompt("read remote task");
	const iterator = deferred.stream[Symbol.asyncIterator]();
	let delivered = false;
	const next = iterator.next().then((value) => {
		delivered = true;
		return value;
	});

	await Promise.resolve();
	assert.equal(delivered, false);

	deferred.release();
	assert.deepEqual(await next, {
		done: false,
		value: {
			type: "user",
			session_id: "",
			message: {
				role: "user",
				content: [{ type: "text", text: "read remote task" }],
			},
			parent_tool_use_id: null,
		},
	});
	assert.deepEqual(await iterator.next(), { done: true, value: undefined });
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
