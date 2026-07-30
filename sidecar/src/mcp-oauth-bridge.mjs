import { chmodSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const AUTH_DIRECTORY_PREFIX = "dcc-claude-mcp-oauth-";
const HEADER_NAME_PATTERN = /^[A-Za-z0-9_-]+$/;
const AUTH_TIMEOUT_SECONDS = "180";

function cloneProjectionWithServers(projection, servers) {
	return {
		servers,
		definitionIds: projection.definitionIds,
		toolPolicies: projection.toolPolicies,
	};
}

export function projectRemoteHttpServersThroughOAuthProxy(
	projection,
	launcher,
) {
	const servers = {};
	let changed = false;
	let serverIndex = 0;

	for (const [serverName, transport] of Object.entries(projection.servers)) {
		if (transport.type !== "http" || !transport.url.startsWith("https://")) {
			servers[serverName] = transport;
			serverIndex += 1;
			continue;
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
		};
		let headerIndex = 0;
		for (const [headerName, headerValue] of Object.entries(
			transport.headers ?? {},
		)) {
			if (!HEADER_NAME_PATTERN.test(headerName)) {
				throw new Error("invalid DCC MCP OAuth proxy header");
			}
			const envName = `DCC_MCP_REMOTE_HEADER_${serverIndex}_${headerIndex}`;
			env[envName] = headerValue;
			args.push("--header", `${headerName}:\${${envName}}`);
			headerIndex += 1;
		}

		servers[serverName] = {
			type: "stdio",
			command: launcher.command,
			args,
			env,
			alwaysLoad: true,
		};
		changed = true;
		serverIndex += 1;
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

	const ensureAuthConfigDir = () => {
		if (authConfigDir) {
			return authConfigDir;
		}
		authConfigDir = mkdtempSync(join(tmpdir(), AUTH_DIRECTORY_PREFIX));
		chmodSync(authConfigDir, 0o700);
		return authConfigDir;
	};

	const cleanup = () => {
		if (!authConfigDir) {
			return;
		}
		const directory = authConfigDir;
		authConfigDir = null;
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
			});
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
