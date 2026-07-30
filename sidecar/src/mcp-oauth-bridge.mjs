import {
	chmodSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const AUTH_DIRECTORY_PREFIX = "dcc-claude-mcp-oauth-";
const HEADER_NAME_PATTERN = /^[A-Za-z0-9_-]+$/;
const AUTH_TIMEOUT_SECONDS = "180";
const MCP_REMOTE_STATE_DIRECTORY = "state";
const OAUTH_STATE_VERSION = 1;
const MAX_OAUTH_STATE_BYTES = 64 * 1024;

function cloneProjectionWithServers(projection, servers) {
	return {
		servers,
		definitionIds: projection.definitionIds,
		toolPolicies: projection.toolPolicies,
		oauthStates: projection.oauthStates,
	};
}

function isRecord(value) {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

function parseOauthState(value) {
	if (
		typeof value !== "string" ||
		Buffer.byteLength(value, "utf8") > MAX_OAUTH_STATE_BYTES
	) {
		throw new Error("invalid DCC MCP OAuth state");
	}
	let parsed;
	try {
		parsed = JSON.parse(value);
	} catch {
		throw new Error("invalid DCC MCP OAuth state");
	}
	if (
		!isRecord(parsed) ||
		parsed.version !== OAUTH_STATE_VERSION ||
		(parsed.clientInfo !== null &&
			parsed.clientInfo !== undefined &&
			!isRecord(parsed.clientInfo)) ||
		!isRecord(parsed.tokens)
	) {
		throw new Error("invalid DCC MCP OAuth state");
	}
	return {
		version: OAUTH_STATE_VERSION,
		clientInfo: parsed.clientInfo ?? null,
		tokens: parsed.tokens,
	};
}

function remoteStateDirectory(authConfigDir) {
	return join(authConfigDir, MCP_REMOTE_STATE_DIRECTORY);
}

function remoteStatePath(authConfigDir, serverHash, filename) {
	return join(remoteStateDirectory(authConfigDir), `${serverHash}_${filename}`);
}

function writePrivateJson(path, value) {
	writeFileSync(path, JSON.stringify(value, null, 2), {
		encoding: "utf8",
		mode: 0o600,
	});
	chmodSync(path, 0o600);
}

function readJsonIfPresent(path) {
	try {
		const raw = readFileSync(path, "utf8");
		if (Buffer.byteLength(raw, "utf8") > MAX_OAUTH_STATE_BYTES) {
			throw new Error("invalid DCC MCP OAuth state");
		}
		const value = JSON.parse(raw);
		if (!isRecord(value)) {
			throw new Error("invalid DCC MCP OAuth state");
		}
		return value;
	} catch (error) {
		if (error?.code === "ENOENT") {
			return null;
		}
		throw new Error("invalid DCC MCP OAuth state");
	}
}

function mcpRemoteServerHash(serverUrl, proxyHeaders) {
	const parts = [serverUrl];
	const keys = Object.keys(proxyHeaders).sort();
	if (keys.length > 0) {
		parts.push(JSON.stringify(proxyHeaders, keys));
	}
	return createHash("md5").update(parts.join("|")).digest("hex");
}

export function projectRemoteHttpServersThroughOAuthProxy(
	projection,
	launcher,
) {
	const servers = {};
	let changed = false;

	for (const [serverName, transport] of Object.entries(projection.servers)) {
		if (transport.type !== "http" || !transport.url.startsWith("https://")) {
			servers[serverName] = transport;
			continue;
		}
		const definitionId = projection.definitionIds[serverName];
		if (typeof definitionId !== "string" || definitionId.trim().length === 0) {
			throw new Error("invalid DCC MCP OAuth proxy identity");
		}

		const args = [
			...launcher.args,
			transport.url,
			"--transport",
			"http-only",
			"--auth-timeout",
			AUTH_TIMEOUT_SECONDS,
			"--silent",
		];
		const env = {
			MCP_REMOTE_CONFIG_DIR: launcher.authConfigDir,
			DCC_MCP_REMOTE_STATE_DIR: remoteStateDirectory(launcher.authConfigDir),
		};
		const proxyHeaders = {};
		for (const [headerName, headerValue] of Object.entries(
			transport.headers ?? {},
		)) {
			if (!HEADER_NAME_PATTERN.test(headerName)) {
				throw new Error("invalid DCC MCP OAuth proxy header");
			}
			const identity = createHash("sha256")
				.update(definitionId)
				.update("\0")
				.update(headerName.toLowerCase())
				.digest("hex")
				.slice(0, 24);
			const envName = `DCC_MCP_REMOTE_HEADER_${identity}`;
			env[envName] = headerValue;
			const proxyValue = `\${${envName}}`;
			proxyHeaders[headerName] = proxyValue;
			args.push("--header", `${headerName}:${proxyValue}`);
		}
		launcher.prepareOauthState?.({
			serverName,
			definitionId,
			serverUrl: transport.url,
			proxyHeaders,
			oauthState: projection.oauthStates?.[serverName],
		});

		servers[serverName] = {
			type: "stdio",
			command: launcher.command,
			args,
			env,
			alwaysLoad: true,
		};
		changed = true;
	}

	return changed ? cloneProjectionWithServers(projection, servers) : projection;
}

function sidecarProxyLauncher(entrypointUrl) {
	if (process.versions.bun) {
		return {
			command: process.execPath,
			args: ["--dcc-mcp-remote-proxy"],
		};
	}
	return {
		command: process.execPath,
		args: [fileURLToPath(entrypointUrl), "--dcc-mcp-remote-proxy"],
	};
}

export function createEphemeralMcpOAuthBridge(entrypointUrl) {
	let authConfigDir = null;
	const oauthServers = new Map();
	const restoredServers = new Set();
	const emittedStates = new Map();

	const ensureAuthConfigDir = () => {
		if (authConfigDir) {
			return authConfigDir;
		}
		authConfigDir = mkdtempSync(join(tmpdir(), AUTH_DIRECTORY_PREFIX));
		chmodSync(authConfigDir, 0o700);
		mkdirSync(remoteStateDirectory(authConfigDir), {
			recursive: true,
			mode: 0o700,
		});
		chmodSync(remoteStateDirectory(authConfigDir), 0o700);
		return authConfigDir;
	};

	const prepareOauthState = ({
		serverName,
		definitionId,
		serverUrl,
		proxyHeaders,
		oauthState,
	}) => {
		if (typeof definitionId !== "string" || definitionId.trim().length === 0) {
			throw new Error("invalid DCC MCP OAuth state");
		}
		const serverHash = mcpRemoteServerHash(serverUrl, proxyHeaders);
		oauthServers.set(serverName, { definitionId, serverHash });
		if (restoredServers.has(serverName) || oauthState === undefined) {
			return;
		}
		const state = parseOauthState(oauthState);
		const directory = remoteStateDirectory(ensureAuthConfigDir());
		if (state.clientInfo) {
			writePrivateJson(
				join(directory, `${serverHash}_client_info.json`),
				state.clientInfo,
			);
		}
		writePrivateJson(
			join(directory, `${serverHash}_tokens.json`),
			state.tokens,
		);
		restoredServers.add(serverName);
		emittedStates.set(serverName, JSON.stringify(state));
	};

	const cleanup = () => {
		if (!authConfigDir) {
			return;
		}
		const directory = authConfigDir;
		authConfigDir = null;
		oauthServers.clear();
		restoredServers.clear();
		emittedStates.clear();
		rmSync(directory, { force: true, recursive: true, maxRetries: 2 });
	};

	process.once("exit", cleanup);

	return {
		project(projection) {
			const hasRemoteHttpServer = Object.values(projection.servers).some(
				(transport) =>
					transport.type === "http" &&
					typeof transport.url === "string" &&
					transport.url.startsWith("https://"),
			);
			if (!hasRemoteHttpServer) {
				return projection;
			}
			return projectRemoteHttpServersThroughOAuthProxy(projection, {
				...sidecarProxyLauncher(entrypointUrl),
				authConfigDir: ensureAuthConfigDir(),
				prepareOauthState,
			});
		},
		collectUpdates() {
			if (!authConfigDir) {
				return [];
			}
			const updates = [];
			for (const [serverName, server] of oauthServers.entries()) {
				const tokens = readJsonIfPresent(
					remoteStatePath(authConfigDir, server.serverHash, "tokens.json"),
				);
				if (!tokens) {
					if (
						emittedStates.has(serverName) &&
						emittedStates.get(serverName) !== null
					) {
						emittedStates.set(serverName, null);
						updates.push({
							definitionId: server.definitionId,
							state: null,
						});
					}
					continue;
				}
				const clientInfo = readJsonIfPresent(
					remoteStatePath(
						authConfigDir,
						server.serverHash,
						"client_info.json",
					),
				);
				const state = JSON.stringify({
					version: OAUTH_STATE_VERSION,
					clientInfo,
					tokens,
				});
				if (emittedStates.get(serverName) === state) {
					continue;
				}
				emittedStates.set(serverName, state);
				updates.push({
					definitionId: server.definitionId,
					state,
				});
			}
			return updates;
		},
		cleanup,
	};
}

export async function runBundledMcpRemoteProxy(argv = process.argv) {
	const modeIndex = argv.indexOf("--dcc-mcp-remote-proxy");
	if (modeIndex < 0) {
		return false;
	}
	const proxyArgs = argv.slice(modeIndex + 1);
	argv.splice(
		0,
		argv.length,
		process.execPath,
		"dcc-mcp-remote-proxy",
		...proxyArgs,
	);
	await import("mcp-remote/dist/proxy.js");
	return true;
}
