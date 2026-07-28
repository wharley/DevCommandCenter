const MAX_SERVER_COUNT = 32;
const MAX_SERVER_NAME_LENGTH = 64;
const MAX_ARGUMENT_COUNT = 128;
const MAX_SECRET_COUNT = 64;
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

export function normalizeDccMcpServers(value) {
	if (!Array.isArray(value) || value.length > MAX_SERVER_COUNT) {
		throw invalidConfiguration();
	}
	const servers = {};
	const definitionIds = {};
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
		servers[entry.name] = normalizeTransport(entry.transport);
		definitionIds[entry.name] = entry.definitionId;
	}
	return { servers, definitionIds };
}

function boundedToolNames(tools) {
	if (!Array.isArray(tools)) {
		return [];
	}
	const names = [];
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
		names.push(name);
		if (names.length === 256) {
			break;
		}
	}
	return names;
}

export function dccMcpQueryOptions(projection) {
	const names = Object.keys(projection.servers);
	if (names.length === 0) {
		return {};
	}
	return { mcpServers: projection.servers };
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
					? boundedToolNames(status.tools)
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
