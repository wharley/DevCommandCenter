import { readDccMcpStatus } from "./mcp-config.mjs";

const DEFAULT_ATTACH_TIMEOUT_MS = 180_000;
const DEFAULT_POLL_INTERVAL_MS = 250;

function userMessage(prompt) {
	return {
		type: "user",
		session_id: "",
		message: {
			role: "user",
			content: [{ type: "text", text: prompt }],
		},
		parent_tool_use_id: null,
	};
}

export function createDeferredUserPrompt(prompt) {
	let settle;
	let settled = false;
	const gate = new Promise((resolve) => {
		settle = resolve;
	});

	return {
		stream: (async function* deferredPrompt() {
			if (await gate) {
				yield userMessage(prompt);
			}
		})(),
		release() {
			if (!settled) {
				settled = true;
				settle(true);
			}
		},
		cancel() {
			if (!settled) {
				settled = true;
				settle(false);
			}
		},
	};
}

function timedOutSnapshot(snapshot) {
	const servers = snapshot.servers.map((server) =>
		server.status === "pending"
			? { ...server, status: "failed", tools: [] }
			: server,
	);
	return {
		failed: servers
			.filter((server) =>
				["failed", "needs-auth"].includes(server.status),
			)
			.map((server) => server.name),
		servers,
	};
}

function sleep(milliseconds) {
	return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

export async function waitForDccMcpReadiness(
	query,
	projection,
	{
		timeoutMs = DEFAULT_ATTACH_TIMEOUT_MS,
		pollIntervalMs = DEFAULT_POLL_INTERVAL_MS,
		onSnapshot,
		now = Date.now,
		wait = sleep,
	} = {},
) {
	const startedAt = now();
	let previousSnapshot = null;

	while (true) {
		const snapshot = await readDccMcpStatus(query, projection);
		const serialized = JSON.stringify(snapshot);
		if (serialized !== previousSnapshot) {
			previousSnapshot = serialized;
			onSnapshot?.(snapshot);
		}
		if (!snapshot.servers.some((server) => server.status === "pending")) {
			return snapshot;
		}

		const elapsed = now() - startedAt;
		if (elapsed >= timeoutMs) {
			const timedOut = timedOutSnapshot(snapshot);
			onSnapshot?.(timedOut);
			return timedOut;
		}
		await wait(Math.min(pollIntervalMs, timeoutMs - elapsed));
	}
}
