const MAX_SERVER_COUNT = 32;
const MAX_SERVER_NAME_LENGTH = 64;
const MAX_ARGUMENT_COUNT = 128;
const MAX_SECRET_COUNT = 64;
const MAX_OAUTH_STATE_LENGTH = 64 * 1024;
const TOOL_NAME_LIMIT = 128;

const STATUS_VALUES = new Set([
	"connected",
	"failed",
	"needs-auth",
	"pending",
	"disabled",
]);

function invalidConfiguration() {
	return new Error("invalid DCC MCP configuration");
}

function isRecord(value) {
	return value !== null && typeof value === "object" && !Array.isArray(value);
}

function validServerName(value) {
	return (
		typeof value === "string" &&
		value.startsWith("dcc-") &&
		value.length <= MAX_SERVER_NAME_LENGTH &&
		/^dcc-[A-Za-z0-9_-]+$/.test(value)
	);
}

function normalizeStringMap(value, namePattern, allowNewlines = false) {
	if (!isRecord(value) || Object.keys(value).length > MAX_SECRET_COUNT) {
		throw invalidConfiguration();
	}
	const normalized = {};
	for (const [name, secret] of Object.entries(value)) {
		if (
			!namePattern.test(name) ||
			typeof secret !== "string" ||
			secret.includes("\0") ||
			(!allowNewlines && (secret.includes("\r") || secret.includes("\n")))
		) {
			throw invalidConfiguration();
		}
		normalized[name] = secret;
	}
	return normalized;
}

function normalizeTransport(value) {
	if (!isRecord(value)) {
		throw invalidConfiguration();
	}
	if (value.type === "stdio") {
		if (
			typeof value.command !== "string" ||
			value.command.trim().length === 0 ||
			value.command.includes("\0") ||
			!Array.isArray(value.args) ||
			value.args.length > MAX_ARGUMENT_COUNT ||
			value.args.some(
				(argument) => typeof argument !== "string" || argument.includes("\0"),
			)
		) {
			throw invalidConfiguration();
		}
		return {
			type: "stdio",
			command: value.command,
			args: [...value.args],
			env: normalizeStringMap(
				value.env,
				/^[A-Za-z_][A-Za-z0-9_]*$/,
				true,
			),
			alwaysLoad: true,
		};
	}
	if (value.type === "http") {
		let url;
		try {
			url = new URL(value.url);
		} catch {
			throw invalidConfiguration();
		}
		if (
			!["http:", "https:"].includes(url.protocol) ||
			url.username ||
			url.password ||
			url.hash
		) {
			throw invalidConfiguration();
		}
		return {
			type: "http",
			url: url.toString(),
			headers: normalizeStringMap(
				value.headers,
				/^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/,
			),
			alwaysLoad: true,
		};
	}
	throw invalidConfiguration();
}

function normalizeToolPolicies(value) {
	if (!Array.isArray(value) || value.length > 256) {
		throw invalidConfiguration();
	}
	const policies = {};
	for (const policy of value) {
		if (
			!isRecord(policy) ||
			typeof policy.toolName !== "string" ||
			policy.toolName.length === 0 ||
			policy.toolName.length > TOOL_NAME_LIMIT ||
			!/^[A-Za-z0-9_.-]+$/.test(policy.toolName) ||
			!["allow", "deny"].includes(policy.decision) ||
			Object.hasOwn(policies, policy.toolName)
		) {
			throw invalidConfiguration();
		}
		policies[policy.toolName] = policy.decision;
	}
	return policies;
}

export function normalizeDccMcpServers(value) {
	if (!Array.isArray(value) || value.length > MAX_SERVER_COUNT) {
		throw invalidConfiguration();
	}
	const servers = {};
	const definitionIds = {};
	const toolPolicies = {};
	const oauthStates = {};
	for (const entry of value) {
		if (
			!isRecord(entry) ||
			typeof entry.definitionId !== "string" ||
			entry.definitionId.trim().length === 0 ||
			!validServerName(entry.name) ||
			Object.hasOwn(servers, entry.name)
		) {
			throw invalidConfiguration();
		}
		const transport = normalizeTransport(entry.transport);
		const oauthState = entry.oauthState;
		if (
			oauthState !== undefined &&
			(typeof oauthState !== "string" ||
				oauthState.length === 0 ||
				oauthState.length > MAX_OAUTH_STATE_LENGTH ||
				oauthState.includes("\0") ||
				transport.type !== "http")
		) {
			throw invalidConfiguration();
		}
		servers[entry.name] = transport;
		definitionIds[entry.name] = entry.definitionId;
		toolPolicies[entry.name] = normalizeToolPolicies(entry.toolPolicies ?? []);
		if (oauthState !== undefined) {
			oauthStates[entry.name] = oauthState;
		}
	}
	return { servers, definitionIds, toolPolicies, oauthStates };
}

export function resolveDccMcpToolPolicy(projection, toolName) {
	if (typeof toolName !== "string") {
		return null;
	}
	for (const serverName of Object.keys(projection.servers)) {
		const prefix = `mcp__${serverName}__`;
		if (toolName.startsWith(prefix)) {
			const serverToolName = toolName.slice(prefix.length);
			return {
				decision:
					projection.toolPolicies[serverName]?.[serverToolName] ?? "ask",
				toolName: serverToolName,
			};
		}
	}
	return null;
}

function boundedToolSummaries(tools) {
	if (!Array.isArray(tools)) {
		return [];
	}
	const summaries = [];
	const seen = new Set();
	for (const tool of tools) {
		const name = tool?.name;
		if (
			typeof name !== "string" ||
			name.length === 0 ||
			name.length > TOOL_NAME_LIMIT ||
			!/^[A-Za-z0-9_.-]+$/.test(name) ||
			seen.has(name)
		) {
			continue;
		}
		seen.add(name);
		const annotations = {};
		if (isRecord(tool.annotations)) {
			if (typeof tool.annotations.readOnly === "boolean") {
				annotations.readOnlyHint = tool.annotations.readOnly;
			}
			if (typeof tool.annotations.destructive === "boolean") {
				annotations.destructiveHint = tool.annotations.destructive;
			}
			if (typeof tool.annotations.openWorld === "boolean") {
				annotations.openWorldHint = tool.annotations.openWorld;
			}
		}
		summaries.push({ name, annotations });
		if (summaries.length === 256) {
			break;
		}
	}
	return summaries;
}

export function dccMcpQueryOptions(projection) {
	const names = Object.keys(projection.servers);
	if (names.length === 0) {
		return {};
	}
	const allowedTools = [];
	const disallowedTools = [];
	for (const serverName of names) {
		for (const [toolName, decision] of Object.entries(
			projection.toolPolicies[serverName] ?? {},
		)) {
			const qualifiedName = `mcp__${serverName}__${toolName}`;
			if (decision === "allow") {
				allowedTools.push(qualifiedName);
			} else if (decision === "deny") {
				disallowedTools.push(qualifiedName);
			}
		}
	}
	return {
		mcpServers: projection.servers,
		...(allowedTools.length > 0 ? { allowedTools } : {}),
		...(disallowedTools.length > 0 ? { disallowedTools } : {}),
	};
}

export async function readDccMcpStatus(query, projection) {
	const names = Object.keys(projection.servers);
	if (names.length === 0) {
		return { failed: [], servers: [] };
	}
	const statuses = await query.mcpServerStatus();
	if (!Array.isArray(statuses)) {
		return failedDccMcpStatus(projection);
	}
	const byName = new Map();
	for (const status of statuses) {
		if (
			!isRecord(status) ||
			typeof status.name !== "string" ||
			!Object.hasOwn(projection.servers, status.name)
		) {
			continue;
		}
		if (byName.has(status.name)) {
			byName.set(status.name, { status: "failed", tools: [] });
			continue;
		}
		const normalizedStatus = STATUS_VALUES.has(status.status)
			? status.status
			: "failed";
		byName.set(status.name, {
			status: normalizedStatus,
			tools:
				normalizedStatus === "connected"
					? boundedToolSummaries(status.tools)
					: [],
		});
	}
	const servers = names.map((name) => {
		const status = byName.get(name) ?? { status: "pending", tools: [] };
		return {
			definitionId: projection.definitionIds[name],
			name,
			status: status.status,
			tools: status.tools,
		};
	});
	const failed = servers
		.filter((server) => ["failed", "needs-auth"].includes(server.status))
		.map((server) => server.name);

	return {
		failed,
		servers,
	};
}

export function failedDccMcpStatus(projection) {
	const servers = Object.keys(projection.servers).map((name) => ({
		definitionId: projection.definitionIds[name],
		name,
		status: "failed",
		tools: [],
	}));
	return {
		failed: servers.map((server) => server.name),
		servers,
	};
}
